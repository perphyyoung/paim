//! 图像 / 提示词相似度：向量化与检索（归一化点积 Top-K）。
//!
//! 存储：`images.vec` / `prompts.vec` 单列 BLOB（f32 LE，写入前已 L2 归一化，点积即余弦），
//! 不另建表、不存模型 / 内容指纹 —— 换 embedding 模型或改预处理规则后，在设置页点「全量重建」即可：
//! - 图像「增量」= `vec IS NULL`：替换图像会产生新 id（旧图软删），无需指纹列；
//! - 提示词「增量」同理，且保存时若 `content` 变化会把该行 `vec` 置空
//!   （见 `prompt_service::update_detail`），因此也不需要内容哈希列。
//!
//! 两侧共用同一套「状态 / 清空 / 写入 / 检索」实现（表名由本模块内部常量给出，不来自入参）；
//! 差异只在「待索引清单」的取数：图像取 `relative_path`（再预处理成 JPEG），提示词取 `content` 原文。
//!
//! 本模块只提供「纯函数 + 单条 SQL」，批量循环留在命令层：万级 × ~0.5s 的长任务
//! **不能长时间持有 DB 锁**，否则主页查询会被整段卡住。

use crate::domain::image_ops;
use crate::infra::error::AppError;
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;

/// 入库与检索统一的预处理长边（改动后必须全量重建）。
pub(crate) const LONG_SIDE: u32 = 1024;
/// 预处理输出 JPEG 质量。
pub(crate) const JPEG_QUALITY: u8 = 85;

/// 两张向量表的名字：写死在模块内（不来自入参），拼进 SQL 不构成注入面。
const IMAGES: &str = "images";
const PROMPTS: &str = "prompts";

/// 索引模式：全量会先清空已有向量（换模型 / 改预处理规则后用）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
pub enum IndexMode {
    Incremental,
    Full,
}

/// 待索引的一张图像。
#[derive(Debug, Clone, serde::Serialize, specta::Type)]
pub struct PendingImage {
    pub id: String,
    pub relative_path: String,
}

/// 待索引的一条提示词（只算 `content` 的向量；`title` 仅用于进度显示与日志）。
#[derive(Debug, Clone, serde::Serialize, specta::Type)]
pub struct PendingPrompt {
    pub id: String,
    pub title: String,
    pub content: String,
}

/// 图像相似检索结果（卡片数据由前端按 id 另取，避免本模块依赖卡片投影）。
#[derive(Debug, Clone, serde::Serialize, specta::Type)]
pub struct ImageHit {
    pub image_id: String,
    pub score: f32,
}

/// 提示词相似检索结果（同上，卡片数据按 `prompt_id` 另取）。
#[derive(Debug, Clone, serde::Serialize, specta::Type)]
pub struct PromptHit {
    pub prompt_id: String,
    pub score: f32,
}

/// 索引 / 检索状态（设置页展示，图像与提示词各算一份）。
#[derive(Debug, Clone, serde::Serialize, specta::Type)]
pub struct SimilarityStatus {
    /// 在用条目总数（is_deleted = 0）
    pub total: i64,
    /// 已有向量的条目数
    pub indexed: i64,
    /// 当前向量维度（无向量时为 0）
    pub dim: usize,
    /// 维度与第一行不一致的行数（换模型后应全量重建）
    pub stale: i64,
}

/// 按入库策略预处理图像：解码（含 webp）→ 长边 1024 → JPEG。
/// 服务端图像解码器不认 webp，必须在本侧转码，否则会被静默丢图（向量与画面无关）。
pub fn prepare_image(path: &Path) -> Result<Vec<u8>, AppError> {
    let img = image_ops::open_image(path)
        .map_err(|e| AppError::Message(format!("无法解码图像 {}：{e}", path.display())))?;
    let resized = image_ops::resize_long_side(&img, LONG_SIDE);
    image_ops::encode_jpeg(&resized, JPEG_QUALITY)
        .map_err(|e| AppError::Message(format!("JPEG 编码失败：{e}")))
}

/// f32 向量 → BLOB（小端）。
pub fn vec_to_blob(v: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(v.len() * 4);
    for x in v {
        out.extend_from_slice(&x.to_le_bytes());
    }
    out
}

/// BLOB → f32 向量（忽略不足 4 字节的尾巴）。
pub fn vec_from_blob(b: &[u8]) -> Vec<f32> {
    b.chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

/// 待索引（增量）图像：按更新时间倒序 —— 用户最近关注的先建好。
pub fn pending(conn: &Connection) -> Result<Vec<PendingImage>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT id, relative_path FROM images
         WHERE is_deleted = 0 AND vec IS NULL
         ORDER BY updated_at DESC",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(PendingImage {
            id: r.get(0)?,
            relative_path: r.get(1)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// 待索引（增量）提示词：按更新时间倒序，只取内容（不含标题 / 翻译 / 备注）。
pub fn pending_prompts(conn: &Connection) -> Result<Vec<PendingPrompt>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT id, title, content FROM prompts
         WHERE is_deleted = 0 AND vec IS NULL
         ORDER BY updated_at DESC",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(PendingPrompt {
            id: r.get(0)?,
            title: r.get(1)?,
            content: r.get(2)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// 清空全部图像向量（全量重建的第一步 / 设置页「清空」）。
pub fn clear_all(conn: &Connection) -> Result<usize, AppError> {
    clear_in(conn, IMAGES)
}

/// 清空全部提示词向量。
pub fn clear_prompts(conn: &Connection) -> Result<usize, AppError> {
    clear_in(conn, PROMPTS)
}

fn clear_in(conn: &Connection, table: &str) -> Result<usize, AppError> {
    Ok(conn.execute(
        &format!("UPDATE {table} SET vec = NULL WHERE vec IS NOT NULL"),
        [],
    )?)
}

/// 写入一张图的向量。
pub fn store(conn: &Connection, image_id: &str, vec: &[f32]) -> Result<(), AppError> {
    store_in(conn, IMAGES, image_id, vec)
}

/// 写入一条提示词的向量。
pub fn store_prompt(conn: &Connection, prompt_id: &str, vec: &[f32]) -> Result<(), AppError> {
    store_in(conn, PROMPTS, prompt_id, vec)
}

fn store_in(conn: &Connection, table: &str, id: &str, vec: &[f32]) -> Result<(), AppError> {
    conn.execute(
        &format!("UPDATE {table} SET vec = ?1 WHERE id = ?2"),
        params![vec_to_blob(vec), id],
    )?;
    Ok(())
}

/// 单张图的向量（未索引 / 已删除时返回 None）。
pub fn vec_of(conn: &Connection, image_id: &str) -> Result<Option<Vec<f32>>, AppError> {
    vec_of_in(conn, IMAGES, image_id)
}

/// 单条提示词的向量（未索引 / 已软删时返回 None）。
pub fn prompt_vec_of(conn: &Connection, prompt_id: &str) -> Result<Option<Vec<f32>>, AppError> {
    vec_of_in(conn, PROMPTS, prompt_id)
}

fn vec_of_in(conn: &Connection, table: &str, id: &str) -> Result<Option<Vec<f32>>, AppError> {
    // 行内 `vec` 可为 NULL：闭包按 Option 取列，再用 optional() 兜「无行」，故需 flatten
    let blob: Option<Vec<u8>> = conn
        .query_row(
            &format!("SELECT vec FROM {table} WHERE id = ?1 AND is_deleted = 0"),
            params![id],
            |r| r.get(0),
        )
        .optional()?
        .flatten();
    Ok(blob.map(|b| vec_from_blob(&b)))
}

/// 图像的相对路径（拼原图绝对路径用；不存在 / 已删除返回 None）。
pub fn relative_path_of(conn: &Connection, image_id: &str) -> Result<Option<String>, AppError> {
    Ok(conn
        .query_row(
            "SELECT relative_path FROM images WHERE id = ?1 AND is_deleted = 0",
            params![image_id],
            |r| r.get(0),
        )
        .optional()?)
}

/// 提示词内容（查询目标未建索引时现场补算用；不存在 / 已软删返回 None）。
pub fn prompt_content_of(conn: &Connection, prompt_id: &str) -> Result<Option<String>, AppError> {
    Ok(conn
        .query_row(
            "SELECT content FROM prompts WHERE id = ?1 AND is_deleted = 0",
            params![prompt_id],
            |r| r.get(0),
        )
        .optional()?)
}

/// 图像索引状态统计。
pub fn status(conn: &Connection) -> Result<SimilarityStatus, AppError> {
    status_in(conn, IMAGES)
}

/// 提示词索引状态统计。
pub fn prompt_status(conn: &Connection) -> Result<SimilarityStatus, AppError> {
    status_in(conn, PROMPTS)
}

fn status_in(conn: &Connection, table: &str) -> Result<SimilarityStatus, AppError> {
    let total: i64 = conn.query_row(
        &format!("SELECT COUNT(*) FROM {table} WHERE is_deleted = 0"),
        [],
        |r| r.get(0),
    )?;
    let indexed: i64 = conn.query_row(
        &format!("SELECT COUNT(*) FROM {table} WHERE is_deleted = 0 AND vec IS NOT NULL"),
        [],
        |r| r.get(0),
    )?;
    let dim_bytes: Option<i64> = conn
        .query_row(
            &format!(
                "SELECT length(vec) FROM {table} WHERE is_deleted = 0 AND vec IS NOT NULL LIMIT 1"
            ),
            [],
            |r| r.get(0),
        )
        .optional()?;
    let dim = dim_bytes.unwrap_or(0) as usize / 4;
    let stale: i64 = if dim == 0 {
        0
    } else {
        conn.query_row(
            &format!(
                "SELECT COUNT(*) FROM {table}
                 WHERE is_deleted = 0 AND vec IS NOT NULL AND length(vec) <> ?1"
            ),
            params![(dim * 4) as i64],
            |r| r.get(0),
        )?
    };
    Ok(SimilarityStatus {
        total,
        indexed,
        dim,
        stale,
    })
}

/// 以给定向量检索最相似的图像（id + 余弦分，按分数降序）。
/// 只比较**维度相同**的向量：换模型后维度不同的旧向量自动跳过（`status.stale` 会提示全量重建）。
/// `safe_only` 与主页「安全模式」一致地过滤 `is_safe = 0` 的图像。
pub fn rank(
    conn: &Connection,
    target: &[f32],
    exclude_id: &str,
    limit: usize,
    min_score: f32,
    safe_only: bool,
) -> Result<Vec<ImageHit>, AppError> {
    let hits = rank_in(
        conn, IMAGES, target, exclude_id, limit, min_score, safe_only,
    )?;
    Ok(hits
        .into_iter()
        .map(|(id, score)| ImageHit {
            image_id: id,
            score,
        })
        .collect())
}

/// 以给定向量检索最相似的提示词（id + 余弦分，按分数降序）。
/// 提示词没有「安全模式」口径，故不过滤 `is_safe`。
pub fn rank_prompts(
    conn: &Connection,
    target: &[f32],
    exclude_id: &str,
    limit: usize,
    min_score: f32,
) -> Result<Vec<PromptHit>, AppError> {
    let hits = rank_in(conn, PROMPTS, target, exclude_id, limit, min_score, false)?;
    Ok(hits
        .into_iter()
        .map(|(id, score)| PromptHit {
            prompt_id: id,
            score,
        })
        .collect())
}

/// 通用检索：维度过滤 + 归一化点积 + 阈值 + Top-K，返回 (id, score) 降序。
fn rank_in(
    conn: &Connection,
    table: &str,
    target: &[f32],
    exclude_id: &str,
    limit: usize,
    min_score: f32,
    safe_only: bool,
) -> Result<Vec<(String, f32)>, AppError> {
    let sql = format!(
        "SELECT id, vec FROM {table}
         WHERE is_deleted = 0 AND vec IS NOT NULL AND id <> ?1 AND length(vec) = ?2{}",
        if safe_only { " AND is_safe = 1" } else { "" }
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params![exclude_id, (target.len() * 4) as i64], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, Vec<u8>>(1)?))
    })?;
    let mut hits: Vec<(String, f32)> = Vec::new();
    for row in rows {
        let (id, blob) = row?;
        let v = vec_from_blob(&blob);
        if v.len() != target.len() {
            continue;
        }
        // 向量写入前已归一化 → 点积即余弦
        let score: f32 = v.iter().zip(target.iter()).map(|(a, b)| a * b).sum();
        if score >= min_score {
            hits.push((id, score));
        }
    }
    hits.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    hits.truncate(limit);
    Ok(hits)
}

#[cfg(test)]
#[path = "similarity_service.test.rs"]
mod tests;
