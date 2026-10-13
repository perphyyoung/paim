//! 图像 / 提示词相似度：向量化与检索（归一化点积 Top-K）。
//!
//! 存储：`images.vec` / `prompts.vec` 单列 BLOB（f32 LE，写入前已 L2 归一化，点积即余弦），
//! 不另建表、不存模型 / 内容指纹 —— 换 embedding 模型或改预处理规则后，在设置页点「全量重建」
//! （或只清掉旧维度的向量，再增量重建）即可：
//! - 图像「增量」= `vec IS NULL`：替换图像会产生新 id（旧图软删），无需指纹列；
//! - 提示词「增量」同理，且保存时仅当 `content` 的**规范化形态**变化（[`canonical_text`]）
//!   才把该行 `vec` 置空（见 `prompt_service::update_detail`），因此也不需要内容哈希列。
//!
//! 两侧共用同一套「状态 / 清空 / 写入 / 检索」实现（表名由本模块内部常量给出，不来自入参）；
//! 差异只在「待索引清单」的取数：图像取 `relative_path`（再预处理成 JPEG），
//! 提示词取 `content` 后过 [`canonical_text`] 规范化再送模型。
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

/// 图像检索结果 + 本次检索的维度信息。
/// `dim_mismatch` 是**本表**维度与查询维度不一致、已被候选筛选排除的条数：被排除的行在打分
/// 阶段看不见，只能在这里数出来，供结果页提醒（0 表示没有不一致）。
#[derive(Debug, Clone, serde::Serialize, specta::Type)]
pub struct SimilarImageHits {
    pub hits: Vec<ImageHit>,
    pub dim: usize,
    pub dim_mismatch: usize,
}

/// 提示词检索结果（口径同 [`SimilarImageHits`]，字段名与语义逐项对称）。
#[derive(Debug, Clone, serde::Serialize, specta::Type)]
pub struct SimilarPromptHits {
    pub hits: Vec<PromptHit>,
    pub dim: usize,
    pub dim_mismatch: usize,
}

/// 单一维度下的向量条数（设置页的维度分布项）。
#[derive(Debug, Clone, serde::Serialize, specta::Type)]
pub struct DimCount {
    pub dim: usize,
    pub count: i64,
}

/// 索引 / 检索状态（设置页展示，图像与提示词各算一份）。
#[derive(Debug, Clone, serde::Serialize, specta::Type)]
pub struct SimilarityStatus {
    /// 在用条目总数（is_deleted = 0）
    pub total: i64,
    /// 已有向量的条目数
    pub indexed: i64,
    /// 已建立向量的**维度分布**（按条数降序，同数按维度升序）。
    /// 刻意不拿「物理第一行」当当前维度：换模型后那个口径会指错维度，
    /// 多维度并存时只报分布，由用户判断清哪个维度 —— 见 docs/开发经验.md 第 4 节
    pub dims: Vec<DimCount>,
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

/// 是否为标点 / 符号性字符：全部 ASCII 标点（`is_ascii_punctuation`，含 `|` `~` `+` `=` 等
/// Unicode 归到 S* 类别、但排版语义上是符号的字符）+ Unicode 标点类别 P*（含中文标点）。
/// emoji 等其他符号（So）不算——它旁边的空白按普通词间空白折叠。
fn is_punct_mark(c: char) -> bool {
    use unicode_categories::UnicodeCategories as _;
    c.is_ascii_punctuation() || c.is_punctuation()
}

/// 提示词向量输入的规范化形态：**库内仍存原文，仅向量链路与失效判定用它**。
///
/// 规则：
/// - 去掉首尾空白；
/// - 删除紧邻标点（[`is_punct_mark`]）的空白整段，如 `"a , b"` → `"a,b"`、`"（ x ）"` → `"（x）"`；
/// - 其余位置的连续空白（含换行）折叠为单个空格，如 `"hair\n blue"` → `"hair blue"`
///   （保留一个空格，不会把相邻单词合并成错误 token）。
///
/// 幂等：`canonical_text(canonical_text(x)) == canonical_text(x)`。
/// 改动规则即改变向量口径，存量向量需在设置页全量重建。
pub fn canonical_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c.is_whitespace() {
            // 吞掉整段连续空白，再按左右邻居决定「折叠为一个空格」还是「整段删除」
            while matches!(chars.peek(), Some(w) if w.is_whitespace()) {
                chars.next();
            }
            let left_punct = out.chars().next_back().is_some_and(is_punct_mark);
            // 右邻是标点 → 删除；段落已到结尾（无右邻）→ 删除（兼去尾空白）；
            // out 为空 = 段首空白 → 删除（兼去首空白）
            let drop = out.is_empty()
                || left_punct
                || match chars.peek() {
                    None => true,
                    Some(r) => is_punct_mark(*r),
                };
            if !drop {
                out.push(' ');
            }
        } else {
            out.push(c);
        }
    }
    out
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
    // 维度分布：一条 GROUP BY 取代了原来「取第一行当维度 + 数 stale」的两句。
    // 多维度并存不再是「谁是当前 / 谁是需要重建」的判断题，而是直接摆出分布（见结构与文档注释）
    let mut stmt = conn.prepare(&format!(
        "SELECT length(vec) / 4, COUNT(*) FROM {table}
         WHERE is_deleted = 0 AND vec IS NOT NULL
         GROUP BY length(vec)
         ORDER BY COUNT(*) DESC, length(vec) ASC"
    ))?;
    let dims = stmt
        .query_map([], |r| {
            Ok(DimCount {
                dim: r.get::<_, i64>(0)? as usize,
                count: r.get(1)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(SimilarityStatus {
        total,
        indexed,
        dims,
    })
}

/// 本表已存向量的维度（**仅当维度唯一时**给值）：作为同一张表内增量期的一致性比对基准。
/// 空表或**已存在多种维度**时返回 None —— 后者属于「已混杂」，交设置页的维度分布 + 按维度清空处理；
/// 不在索引期终止（否则「旧维度」本身就不唯一，取哪一行当基准都是随机的）。
/// 只比**同一张表**：图像只与图像比、提示词只与提示词比（允许两表维度不同，见 docs/开发经验.md 第 4 节）。
pub fn image_stored_dim(conn: &Connection) -> Result<Option<usize>, AppError> {
    stored_dim_in(conn, IMAGES)
}

/// 提示词表已存向量的维度（口径同上）。
pub fn prompt_stored_dim(conn: &Connection) -> Result<Option<usize>, AppError> {
    stored_dim_in(conn, PROMPTS)
}

fn stored_dim_in(conn: &Connection, table: &str) -> Result<Option<usize>, AppError> {
    let (distinct, min_bytes): (i64, Option<i64>) = conn.query_row(
        &format!(
            "SELECT COUNT(DISTINCT length(vec)), MIN(length(vec)) FROM {table}
             WHERE is_deleted = 0 AND vec IS NOT NULL"
        ),
        [],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    if distinct == 1 {
        Ok(min_bytes.map(|b| b as usize / 4))
    } else {
        Ok(None)
    }
}

/// 新向量与本表已存维度是否冲突：冲突即应**终止**索引（继续写只会把混杂越铺越广）。
/// 纯函数，两域共用（图像侧与提示词侧各一份调用）。
pub fn dim_conflict(stored: Option<usize>, new: usize) -> bool {
    matches!(stored, Some(d) if d != new)
}

/// 清空指定维度的图像向量（含已软删行：否则回收站恢复出来的旧向量又成了不一致源）。
pub fn clear_image_dim(conn: &Connection, dim: usize) -> Result<usize, AppError> {
    clear_dim_in(conn, IMAGES, dim)
}

/// 清空指定维度的提示词向量（口径同上）。
pub fn clear_prompt_dim(conn: &Connection, dim: usize) -> Result<usize, AppError> {
    clear_dim_in(conn, PROMPTS, dim)
}

fn clear_dim_in(conn: &Connection, table: &str, dim: usize) -> Result<usize, AppError> {
    Ok(conn.execute(
        &format!("UPDATE {table} SET vec = NULL WHERE vec IS NOT NULL AND length(vec) = ?1"),
        params![(dim * 4) as i64],
    )?)
}

/// 短锁阶段产出的候选行：`(id, 已归一化向量的原始 BLOB)`。
/// 解码 + 点积 + 排序都在锁外做——万级向量时那才是耗时大头（见 docs/lessons.md 第 28 节）。
pub type RankRow = (String, Vec<u8>);

/// **短锁阶段**：取图像表里参与打分的候选（维度过滤下推 SQL，避免把无用的 BLOB 拉进内存），
/// 同时数出**本表维度不一致的条数**（结果页据此提醒：被过滤掉的行在打分阶段是看不见的）。
/// `safe_only` 与主页「安全模式」一致地过滤 `is_safe = 0`；`exclude_id` 传空串表示不排除任何行。
pub fn image_rank_candidates(
    conn: &Connection,
    dim_len: usize,
    exclude_id: &str,
    safe_only: bool,
) -> Result<(Vec<RankRow>, usize), AppError> {
    load_candidates(conn, IMAGES, dim_len, exclude_id, safe_only)
}

/// **短锁阶段**：取提示词表里参与打分的候选（提示词没有「安全模式」口径）。
pub fn prompt_rank_candidates(
    conn: &Connection,
    dim_len: usize,
    exclude_id: &str,
) -> Result<(Vec<RankRow>, usize), AppError> {
    load_candidates(conn, PROMPTS, dim_len, exclude_id, false)
}

fn load_candidates(
    conn: &Connection,
    table: &str,
    dim_len: usize,
    exclude_id: &str,
    safe_only: bool,
) -> Result<(Vec<RankRow>, usize), AppError> {
    let safe = if safe_only { " AND is_safe = 1" } else { "" };
    let sql = format!(
        "SELECT id, vec FROM {table}
         WHERE is_deleted = 0 AND vec IS NOT NULL AND id <> ?1 AND length(vec) = ?2{safe}"
    );
    let mut stmt = conn.prepare(&sql)?;
    let dim_bytes = (dim_len * 4) as i64;
    let rows = stmt.query_map(params![exclude_id, dim_bytes], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, Vec<u8>>(1)?))
    })?;
    let rows = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    // 同一次短锁里顺带数不一致的行（多一句 COUNT，避免为它再进一次锁）
    let mismatch: i64 = conn.query_row(
        &format!(
            "SELECT COUNT(*) FROM {table}
             WHERE is_deleted = 0 AND vec IS NOT NULL AND length(vec) <> ?1{safe}"
        ),
        params![dim_bytes],
        |r| r.get(0),
    )?;
    Ok((rows, mismatch as usize))
}

/// **无锁阶段**：归一化点积 + 阈值 + Top-K，返回 (id, score) 降序。
/// 向量写入前已归一化，故点积即余弦；维度不符的行跳过（换模型后的旧向量）。
pub fn score_ranked(
    rows: &[RankRow],
    target: &[f32],
    limit: usize,
    min_score: f32,
) -> Vec<(String, f32)> {
    let mut hits: Vec<(String, f32)> = Vec::new();
    for (id, blob) in rows {
        let v = vec_from_blob(blob);
        if v.len() != target.len() {
            continue;
        }
        let score: f32 = v.iter().zip(target.iter()).map(|(a, b)| a * b).sum();
        if score >= min_score {
            hits.push((id.clone(), score));
        }
    }
    hits.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    hits.truncate(limit);
    hits
}

#[cfg(test)]
#[path = "similarity_service.test.rs"]
mod tests;
