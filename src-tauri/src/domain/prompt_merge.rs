//! 提示词合并：把两条高度相似的提示词合并为一条新提示词。
//! 语义（与前端合并弹窗的预览口径一致）：
//! - 新建 C：标题用新 id（与无标题新建一致），content_translate 置空，
//!   note 取两侧去重后按 A、B 顺序拼接；
//! - 关联图像 / 标签为两侧**并集复制**（不移动）：A/B 软删后关系行仍在，
//!   从回收站恢复 A、B 再删 C 即可完整撤销，故允许 C 与软删的 A/B 暂时共享图像；
//! - is_favorite 取 OR，is_safe 取 AND（任一标敏感则合并为敏感，保守口径）；
//! - A、B 随后软删（不删关联行，与 `prompt_service::remove` 的既有语义一致）。
//! 全程单事务，任一步失败整体回滚。

use crate::domain::prompt_service::{self, Prompt};
use rusqlite::{Connection, OptionalExtension, Result};
use serde::Serialize;

#[derive(Debug, Serialize, specta::Type)]
pub struct MergePromptSide {
    pub content: String,
    pub note: String,
    /// 该提示词首图的图像 id（与卡片缩略图同口径：取第一张**有缩略图**的关联未删除图像）。
    /// 供合并弹窗 hover 缩略图时按 id 取原图；无可用图像为 None。
    pub first_image_id: Option<String>,
}

#[derive(Debug, Serialize, specta::Type)]
pub struct MergePromptsPreview {
    pub a: MergePromptSide,
    pub b: MergePromptSide,
    /// 两侧标签并集（标签名，按名排序）
    pub tag_names: Vec<String>,
    /// 去重后的关联图像总数
    pub image_total: i64,
    pub image_a: i64,
    pub image_b: i64,
    /// 两侧共有的关联图像数
    pub image_shared: i64,
    /// 合并后的 note（两侧去重空行拼接），直接展示给用户确认
    pub merged_note: String,
    pub is_favorite: bool,
    pub is_safe: bool,
}

/// 取一条「存在且未软删」的提示词，否则报错。
fn load_active(conn: &Connection, id: &str, role: &str) -> Result<Prompt> {
    let prompt = prompt_service::get_by_id(conn, id)?
        .ok_or_else(|| rusqlite::Error::InvalidParameterName(format!("{role}提示词不存在")))?;
    if prompt.is_deleted {
        return Err(rusqlite::Error::InvalidParameterName(format!(
            "{role}提示词已在回收站，不能合并"
        )));
    }
    Ok(prompt)
}

/// 合并预览所需的两侧内容、note、标签并集、关联图像计数与合并标记。
pub fn preview(conn: &Connection, a_id: &str, b_id: &str) -> Result<MergePromptsPreview> {
    if a_id == b_id {
        return Err(rusqlite::Error::InvalidParameterName(
            "不能与自身合并".to_string(),
        ));
    }
    let a = load_active(conn, a_id, "源提示词1")?;
    let b = load_active(conn, b_id, "源提示词2")?;

    let tag_names: Vec<String> = {
        let mut stmt = conn.prepare(
            "SELECT DISTINCT t.name
             FROM prompt_tags t
             JOIN prompt_tag_relations r ON r.tag_id = t.id
             WHERE r.prompt_id IN (?1, ?2)
             ORDER BY t.name",
        )?;
        let rows = stmt.query_map(rusqlite::params![a_id, b_id], |row| row.get(0))?;
        rows.collect::<Result<Vec<_>>>()?
    };

    let image_a = count_images(conn, a_id)?;
    let image_b = count_images(conn, b_id)?;
    let image_total = conn.query_row(
        "SELECT COUNT(DISTINCT image_id) FROM prompt_image_relations WHERE prompt_id IN (?1, ?2)",
        rusqlite::params![a_id, b_id],
        |row| row.get(0),
    )?;

    let merged_note = merge_notes(&a.note, &b.note);
    Ok(MergePromptsPreview {
        a: MergePromptSide {
            content: a.content,
            note: a.note,
            first_image_id: first_image_id(conn, a_id)?,
        },
        b: MergePromptSide {
            content: b.content,
            note: b.note,
            first_image_id: first_image_id(conn, b_id)?,
        },
        tag_names,
        image_total,
        image_a,
        image_b,
        image_shared: image_a + image_b - image_total,
        merged_note,
        is_favorite: a.is_favorite || b.is_favorite,
        is_safe: a.is_safe && b.is_safe,
    })
}

/// 首图 id：与 `prompt_service::thumbs_for` 同口径——按 sort_order/rowid 取第一张
/// 有缩略图（thumbnail_path 非 NULL）的关联未删除图像；无则 None。
fn first_image_id(conn: &Connection, prompt_id: &str) -> Result<Option<String>> {
    conn.query_row(
        "SELECT img.id
         FROM prompt_image_relations pir
         JOIN images img ON img.id = pir.image_id
         WHERE pir.prompt_id = ?1 AND img.is_deleted = 0 AND img.thumbnail_path IS NOT NULL
         ORDER BY pir.sort_order, pir.rowid
         LIMIT 1",
        rusqlite::params![prompt_id],
        |row| row.get(0),
    )
    .optional()
}

fn count_images(conn: &Connection, prompt_id: &str) -> Result<i64> {
    conn.query_row(
        "SELECT COUNT(*) FROM prompt_image_relations WHERE prompt_id = ?1",
        rusqlite::params![prompt_id],
        |row| row.get(0),
    )
}

/// note 合并：两侧各 trim，去重（完全相同只留一份），按 A、B 顺序空行连接。
fn merge_notes(a: &str, b: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for note in [a, b] {
        let note = note.trim();
        if !note.is_empty() && !parts.contains(&note) {
            parts.push(note);
        }
    }
    parts.join("\n\n")
}

/// 执行合并：新建 C（标题为新 id、译文置空、note 合并），复制图像/标签并集，软删 A、B。
pub fn merge(conn: &Connection, a_id: &str, b_id: &str, content: &str) -> Result<Prompt> {
    if a_id == b_id {
        return Err(rusqlite::Error::InvalidParameterName(
            "不能与自身合并".to_string(),
        ));
    }
    let a = load_active(conn, a_id, "源提示词1")?;
    let b = load_active(conn, b_id, "源提示词2")?;

    let tx = conn.unchecked_transaction()?;

    // 无标题新建：标题取新 id（与 create_prompt 不传标题的既有规则一致）；content_translate 默认空。
    let created = prompt_service::create(&tx, content, None)?;
    let merged_note = merge_notes(&a.note, &b.note);
    tx.execute(
        "UPDATE prompts
         SET note = ?1,
             is_favorite = ?2,
             is_safe = ?3
         WHERE id = ?4",
        rusqlite::params![
            merged_note,
            a.is_favorite || b.is_favorite,
            a.is_safe && b.is_safe,
            created.id
        ],
    )?;

    // 关联图像并集：A 先 B 后，同 sort_order 下按新表 rowid 保序；重复图像 OR IGNORE 去重。
    tx.execute(
        "INSERT OR IGNORE INTO prompt_image_relations (prompt_id, image_id, sort_order)
         SELECT ?1, image_id, sort_order FROM prompt_image_relations WHERE prompt_id = ?2
         ORDER BY sort_order, rowid",
        rusqlite::params![created.id, a_id],
    )?;
    tx.execute(
        "INSERT OR IGNORE INTO prompt_image_relations (prompt_id, image_id, sort_order)
         SELECT ?1, image_id, sort_order FROM prompt_image_relations WHERE prompt_id = ?2
         ORDER BY sort_order, rowid",
        rusqlite::params![created.id, b_id],
    )?;
    // 标签并集：主键 (prompt_id, tag_id) 天然去重。
    tx.execute(
        "INSERT OR IGNORE INTO prompt_tag_relations (prompt_id, tag_id)
         SELECT ?1, tag_id FROM prompt_tag_relations WHERE prompt_id IN (?2, ?3)",
        rusqlite::params![created.id, a_id, b_id],
    )?;

    // 软删 A、B（与 prompt_service::remove 同口径：置删除位并 touch 关联图像的 updated_at）。
    for old_id in [a_id, b_id] {
        tx.execute(
            "UPDATE prompts
             SET is_deleted = 1,
                 deleted_at = strftime('%Y-%m-%dT%H:%M:%fZ','now'),
                 updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now')
             WHERE id = ?1",
            rusqlite::params![old_id],
        )?;
    }
    tx.execute(
        "UPDATE images SET updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now')
         WHERE id IN (SELECT image_id FROM prompt_image_relations WHERE prompt_id IN (?1, ?2))",
        rusqlite::params![a_id, b_id],
    )?;

    tx.commit()?;
    prompt_service::get_by_id(conn, &created.id)?
        .ok_or_else(|| rusqlite::Error::InvalidParameterName("合并后的提示词不存在".to_string()))
}

#[cfg(test)]
#[path = "prompt_merge.test.rs"]
mod tests;
