//! 图像相似度：向量化与检索（归一化点积 Top-K）。
//!
//! 存储：`images.vec` 单列 BLOB（f32 LE，写入前已 L2 归一化，点积即余弦），不另建表、
//! 不存模型/预处理指纹 —— 换 embedding 模型或改预处理规则后，在设置页点「全量重建」即可；
//! 「增量」= `vec IS NULL`。替换图像会产生新 id（旧图软删），因此不需要手工失效。
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

/// 相似检索结果（卡片数据由前端按 id 另取，避免本模块依赖卡片投影）。
#[derive(Debug, Clone, serde::Serialize, specta::Type)]
pub struct SimilarHit {
    pub image_id: String,
    pub score: f32,
}

/// 索引 / 检索状态（设置页展示）。
#[derive(Debug, Clone, serde::Serialize, specta::Type)]
pub struct SimilarityStatus {
    /// 在用图像总数（is_deleted = 0）
    pub total: i64,
    /// 已有向量的图像数
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

/// 清空全部向量（全量重建的第一步 / 设置页「清空」）。
pub fn clear_all(conn: &Connection) -> Result<usize, AppError> {
    Ok(conn.execute("UPDATE images SET vec = NULL WHERE vec IS NOT NULL", [])?)
}

/// 写入一张图的向量。
pub fn store(conn: &Connection, image_id: &str, vec: &[f32]) -> Result<(), AppError> {
    conn.execute(
        "UPDATE images SET vec = ?1 WHERE id = ?2",
        params![vec_to_blob(vec), image_id],
    )?;
    Ok(())
}

/// 单张图的向量（未索引 / 已删除时返回 None）。
pub fn vec_of(conn: &Connection, image_id: &str) -> Result<Option<Vec<f32>>, AppError> {
    // 行内 `vec` 可为 NULL：闭包按 Option 取列，再用 optional() 兜「无行」，故需 flatten
    let blob: Option<Vec<u8>> = conn
        .query_row(
            "SELECT vec FROM images WHERE id = ?1 AND is_deleted = 0",
            params![image_id],
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

/// 状态统计。
pub fn status(conn: &Connection) -> Result<SimilarityStatus, AppError> {
    let total: i64 = conn.query_row(
        "SELECT COUNT(*) FROM images WHERE is_deleted = 0",
        [],
        |r| r.get(0),
    )?;
    let indexed: i64 = conn.query_row(
        "SELECT COUNT(*) FROM images WHERE is_deleted = 0 AND vec IS NOT NULL",
        [],
        |r| r.get(0),
    )?;
    let dim_bytes: Option<i64> = conn
        .query_row(
            "SELECT length(vec) FROM images WHERE is_deleted = 0 AND vec IS NOT NULL LIMIT 1",
            [],
            |r| r.get(0),
        )
        .optional()?;
    let dim = dim_bytes.unwrap_or(0) as usize / 4;
    let stale: i64 = if dim == 0 {
        0
    } else {
        conn.query_row(
            "SELECT COUNT(*) FROM images
             WHERE is_deleted = 0 AND vec IS NOT NULL AND length(vec) <> ?1",
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
) -> Result<Vec<SimilarHit>, AppError> {
    let sql = format!(
        "SELECT id, vec FROM images
         WHERE is_deleted = 0 AND vec IS NOT NULL AND id <> ?1 AND length(vec) = ?2{}",
        if safe_only { " AND is_safe = 1" } else { "" }
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params![exclude_id, (target.len() * 4) as i64], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, Vec<u8>>(1)?))
    })?;
    let mut hits: Vec<SimilarHit> = Vec::new();
    for row in rows {
        let (id, blob) = row?;
        let v = vec_from_blob(&blob);
        if v.len() != target.len() {
            continue;
        }
        // 向量写入前已归一化 → 点积即余弦
        let score: f32 = v.iter().zip(target.iter()).map(|(a, b)| a * b).sum();
        if score >= min_score {
            hits.push(SimilarHit {
                image_id: id,
                score,
            });
        }
    }
    hits.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    hits.truncate(limit);
    Ok(hits)
}

#[cfg(test)]
#[path = "similarity_service.test.rs"]
mod tests;
