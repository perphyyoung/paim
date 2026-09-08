//! 提示词领域服务：承载提示词的业务规则。
//! 领域层不感知 Tauri，通过注入的事务获取连接访问数据。
//! 表结构与字段名与 prompt-manager 一致。

use crate::domain::tag_manager::{tags_by_owner, TagDomain};
use crate::infra::error::AppError;
use rusqlite::{Connection, OptionalExtension, Result};

use serde::Serialize;

#[derive(Debug, Serialize, Clone, specta::Type)]
pub struct Prompt {
    pub id: String,
    pub title: String,
    pub content: String,
    pub content_translate: String,
    pub created_at: String,
    pub updated_at: String,
    pub is_deleted: bool,
    pub deleted_at: Option<String>,
    pub is_favorite: bool,
    pub is_safe: bool,
    pub note: String,
}

fn validate_content(content: &str) -> Result<String> {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return Err(rusqlite::Error::InvalidParameterName(
            "提示词内容不能为空".to_string(),
        ));
    }
    Ok(trimmed.to_string())
}

pub fn create(conn: &Connection, content: &str, title: Option<String>) -> Result<Prompt> {
    let content = validate_content(content)?;
    let mut title = title
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .unwrap_or_default();
    let id = crate::infra::db::gen_id(crate::infra::db::PROMPT_ID_PREFIX);
    // 与 pm 一致：未提供标题时，用提示词 id 作为标题
    if title.is_empty() {
        title = id.clone();
    }

    // 单条 INSERT 原子生效，不开事务；调用方需要多步原子性时在外层自行包事务
    conn.execute(
        "INSERT INTO prompts(id, title, content) VALUES (?1, ?2, ?3)",
        rusqlite::params![id, title, content],
    )?;
    Ok(get_by_id(conn, &id)?.expect("inserted prompt must exist"))
}

pub fn get_by_id(conn: &Connection, id: &str) -> Result<Option<Prompt>> {
    let mut stmt = conn.prepare(
        "SELECT id, title, content, content_translate, created_at, updated_at, is_deleted, deleted_at, is_favorite, is_safe, note
         FROM prompts WHERE id = ?1",
    )?;
    let mut rows = stmt.query_map(rusqlite::params![id], row_to_prompt)?;
    rows.next().transpose()
}

pub fn list(conn: &Connection) -> Result<Vec<Prompt>> {
    let mut stmt = conn.prepare(
        "SELECT id, title, content, content_translate, created_at, updated_at, is_deleted, deleted_at, is_favorite, is_safe, note
         FROM prompts WHERE is_deleted = 0 ORDER BY updated_at DESC",
    )?;
    let rows = stmt.query_map([], row_to_prompt)?;
    rows.collect()
}

/// 软删除：标记为已删除（与图像回收站机制一致）。
/// 软删除是明显的更新操作，同步刷新 updated_at。
pub fn remove(conn: &Connection, id: &str) -> Result<()> {
    conn.execute(
        "UPDATE prompts
         SET is_deleted = 1,
             deleted_at = strftime('%Y-%m-%dT%H:%M:%fZ','now'),
             updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now')
         WHERE id = ?1",
        rusqlite::params![id],
    )?;
    Ok(())
}

/// 列出回收站中的提示词（已软删除），按删除时间倒序。
pub fn list_trashed(conn: &Connection) -> Result<Vec<Prompt>> {
    let mut stmt = conn.prepare(
        "SELECT id, title, content, content_translate, created_at, updated_at, is_deleted, deleted_at, is_favorite, is_safe, note
         FROM prompts WHERE is_deleted = 1 ORDER BY deleted_at DESC",
    )?;
    let rows = stmt.query_map([], row_to_prompt)?;
    rows.collect()
}

/// 恢复软删除的提示词。恢复是明显的更新操作，同步刷新 updated_at。
pub fn restore(conn: &Connection, id: &str) -> Result<Option<Prompt>> {
    conn.execute(
        "UPDATE prompts
         SET is_deleted = 0, deleted_at = NULL,
             updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now')
         WHERE id = ?1",
        rusqlite::params![id],
    )?;
    get_by_id(conn, id)
}

/// 彻底删除提示词（关联关系随外键级联删除）。
/// 级联删除视为隐式解绑，同步刷新关联图像的 updated_at。
pub fn purge(conn: &Connection, id: &str) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    let related_image_ids: Vec<String> = {
        let mut stmt =
            tx.prepare("SELECT image_id FROM prompt_image_relations WHERE prompt_id = ?1")?;
        let rows = stmt.query_map(rusqlite::params![id], |r| r.get(0))?;
        rows.collect::<Result<Vec<String>>>()?
    };
    tx.execute("DELETE FROM prompts WHERE id = ?1", rusqlite::params![id])?;
    if !related_image_ids.is_empty() {
        let placeholders = vec!["?"; related_image_ids.len()].join(",");
        tx.execute(
            &format!(
                "UPDATE images SET updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now')
                 WHERE id IN ({placeholders})"
            ),
            rusqlite::params_from_iter(related_image_ids.iter()),
        )?;
    }
    tx.commit()?;
    Ok(())
}

/// 恢复全部回收站提示词，返回恢复数量。恢复是明显的更新操作，同步刷新 updated_at。
pub fn restore_all(conn: &Connection) -> Result<usize> {
    Ok(conn.execute(
        "UPDATE prompts
         SET is_deleted = 0, deleted_at = NULL,
             updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now')
         WHERE is_deleted = 1",
        [],
    )?)
}

/// 清空回收站提示词（关联关系随外键级联删除），返回清理数量。
/// 级联删除视为隐式解绑，同步刷新关联图像的 updated_at。
pub fn empty_trash(conn: &Connection) -> Result<usize> {
    let tx = conn.unchecked_transaction()?;
    let count = tx.execute("DELETE FROM prompts WHERE is_deleted = 1", [])?;
    // 关联行已随删除级联消失，只能刷全部图像（回收站清空本身低频，可接受）
    tx.execute(
        "UPDATE images SET updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now')
         WHERE id IN (SELECT image_id FROM prompt_image_relations)",
        [],
    )?;
    tx.commit()?;
    Ok(count)
}

/// 更新提示词详情字段（标题/内容/翻译/备注/收藏/安全）。仅更新传入 `Some` 的字段，未传的字段不动。
/// 非空校验（标题/内容必填）与逐字段脏检查由前端负责，前端只把真正变化的字段传进来；后端不再校验、不再比较。
/// `Some("")` 表示显式清空（翻译/备注允许），与「不更新」的 `None` 语义区分。
pub fn update_detail(
    conn: &Connection,
    id: &str,
    title: Option<String>,
    content: Option<String>,
    content_translate: Option<String>,
    note: Option<String>,
    is_favorite: Option<bool>,
    is_safe: Option<bool>,
) -> std::result::Result<Option<Prompt>, AppError> {
    let mut sets: Vec<String> = Vec::new();
    let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
    if let Some(v) = title {
        sets.push("title = ?".into());
        params.push(Box::new(v.trim().to_string()));
    }
    if let Some(v) = content {
        sets.push("content = ?".into());
        params.push(Box::new(v.trim().to_string()));
    }
    if let Some(v) = content_translate {
        sets.push("content_translate = ?".into());
        params.push(Box::new(v));
    }
    if let Some(v) = note {
        sets.push("note = ?".into());
        params.push(Box::new(v));
    }
    if let Some(v) = is_favorite {
        sets.push("is_favorite = ?".into());
        params.push(Box::new(v as i64));
    }
    if let Some(v) = is_safe {
        sets.push("is_safe = ?".into());
        params.push(Box::new(v as i64));
    }

    // 没有任何字段需要更新：不写库，updated_at 保持不变
    if sets.is_empty() {
        return Ok(get_by_id(conn, id)?);
    }

    sets.push("updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now')".into());
    let sql = format!("UPDATE prompts SET {} WHERE id = ?", sets.join(", "));
    params.push(Box::new(id.to_string()));
    conn.execute(
        &sql,
        rusqlite::params_from_iter(params.iter().map(|b| b.as_ref())),
    )?;
    Ok(get_by_id(conn, id)?)
}

/// 提示词关联的（未删除）图像及其标签，供详情页图像网格展示。
#[derive(Debug, serde::Serialize, Clone, specta::Type)]
pub struct RelatedImage {
    pub id: String,
    pub file_name: String,
    /// 原图像绝对路径（前端配合 convertFileSrc 加载）。
    pub src: String,
    pub tags: Vec<String>,
}

/// 返回一个提示词关联的（未删除）图像列表：id、文件名、原图绝对路径、标签。
pub fn list_related_images(
    conn: &Connection,
    app: &tauri::AppHandle,
    prompt_id: &str,
) -> Result<Vec<RelatedImage>> {
    list_related_images_with(conn, &crate::infra::db::data_dir(app), prompt_id)
}

/// 与 `list_related_images` 相同，但数据目录由调用方注入（便于脱离 Tauri 单测）。
pub fn list_related_images_with(
    conn: &Connection,
    data_dir: &std::path::Path,
    prompt_id: &str,
) -> Result<Vec<RelatedImage>> {
    let mut stmt = conn.prepare(
        "SELECT img.id, img.file_name, img.relative_path
         FROM prompt_image_relations pir
         JOIN images img ON img.id = pir.image_id
         WHERE pir.prompt_id = ?1 AND img.is_deleted = 0
         ORDER BY pir.sort_order, pir.rowid",
    )?;
    let rows = stmt.query_map(rusqlite::params![prompt_id], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, Option<String>>(2)?,
        ))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (id, file_name, src_rel) = row?;
        out.push(RelatedImage {
            id,
            file_name,
            src: src_rel
                .map(|rel| data_dir.join(&rel).to_string_lossy().into_owned())
                .unwrap_or_default(),
            tags: Vec::new(),
        });
    }
    let ids: Vec<&str> = out.iter().map(|i| i.id.as_str()).collect();
    let mut tags = tags_by_owner(conn, TagDomain::Image, &ids)?;
    for img in out.iter_mut() {
        img.tags = tags.remove(&img.id).unwrap_or_default();
    }
    Ok(out)
}

/// 设为首图：将关联行的 sort_order 置为当前最小值 - 1（借鉴标签组固定首位的模式）。
/// 对齐 pm 的"设为首图"：读取侧按 sort_order 升序，最小者即首图；存量全 0 数据置 -1 自然生效。
pub fn set_prompt_first_image(
    conn: &Connection,
    prompt_id: &str,
    image_id: &str,
) -> std::result::Result<(), AppError> {
    let tx = conn.unchecked_transaction()?;
    let related = tx
        .query_row(
            "SELECT 1 FROM prompt_image_relations WHERE prompt_id = ?1 AND image_id = ?2",
            rusqlite::params![prompt_id, image_id],
            |_| Ok(()),
        )
        .optional()
        .map_err(AppError::from)?;
    if related.is_none() {
        return Err(AppError::Message(format!(
            "图像 {image_id} 未关联提示词 {prompt_id}"
        )));
    }
    tx.execute(
        "UPDATE prompt_image_relations
         SET sort_order = (SELECT MIN(sort_order) - 1 FROM prompt_image_relations WHERE prompt_id = ?1)
         WHERE prompt_id = ?1 AND image_id = ?2",
        rusqlite::params![prompt_id, image_id],
    )
    .map_err(AppError::from)?;
    // 首图变化视为提示词内容变更，同步更新 updated_at（列表按时间排序时封面顺序随之生效）
    tx.execute(
        "UPDATE prompts SET updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id = ?1",
        rusqlite::params![prompt_id],
    )
    .map_err(AppError::from)?;
    tx.commit()?;
    Ok(())
}

/// 校验提示词存在，不存在则报错（避免静默创建孤立标签关联）。
fn ensure_prompt_exists(tx: &rusqlite::Transaction, id: &str) -> std::result::Result<(), AppError> {
    let found = tx
        .query_row(
            "SELECT 1 FROM prompts WHERE id = ?1",
            rusqlite::params![id],
            |_| Ok(()),
        )
        .optional()?;
    if found.is_none() {
        return Err(AppError::Message(format!("提示词 {id} 不存在")));
    }
    Ok(())
}

/// 为单个提示词添加一个标签（不存在则创建，关联存在则忽略），返回关联的标签。
pub fn add_prompt_tag(
    conn: &Connection,
    id: &str,
    name: &str,
) -> std::result::Result<Vec<(i64, String)>, AppError> {
    let tx = conn.unchecked_transaction()?;
    ensure_prompt_exists(&tx, id)?;
    let tag_id = get_or_create_prompt_tag(&tx, name)?;
    tx.execute(
        "INSERT OR IGNORE INTO prompt_tag_relations(prompt_id, tag_id) VALUES (?1, ?2)",
        rusqlite::params![id, tag_id],
    )?;
    // 标签变化视为提示词内容变更，同步 updated_at（与图像侧 add_image_tag 对称）
    tx.execute(
        "UPDATE prompts SET updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id = ?1",
        rusqlite::params![id],
    )?;
    tx.commit()?;
    Ok(vec![(tag_id, name.to_string())])
}

/// 为多个提示词批量添加同一个标签（单事务）：一次为多个项目打同一个标签。
pub fn batch_add_prompt_tag(
    conn: &Connection,
    ids: &[&str],
    name: &str,
) -> std::result::Result<(), AppError> {
    let tx = conn.unchecked_transaction()?;
    for id in ids {
        ensure_prompt_exists(&tx, id)?;
    }
    let tag_id = get_or_create_prompt_tag(&tx, name)?;
    for id in ids {
        tx.execute(
            "INSERT OR IGNORE INTO prompt_tag_relations(prompt_id, tag_id) VALUES (?1, ?2)",
            rusqlite::params![id, tag_id],
        )?;
        tx.execute(
            "UPDATE prompts SET updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id = ?1",
            rusqlite::params![id],
        )?;
    }
    tx.commit()?;
    Ok(())
}

/// 获取提示词标签 id，不存在则创建（供单标签与批量场景复用）。
fn get_or_create_prompt_tag(tx: &rusqlite::Transaction, name: &str) -> Result<i64> {
    match tx
        .query_row(
            "SELECT id FROM prompt_tags WHERE name = ?1",
            rusqlite::params![name],
            |r| r.get(0),
        )
        .optional()?
    {
        Some(tid) => Ok(tid),
        None => {
            tx.execute(
                "INSERT INTO prompt_tags(name) VALUES (?1)",
                rusqlite::params![name],
            )?;
            Ok(tx.last_insert_rowid())
        }
    }
}

/// 移除提示词的一个标签关联。
pub fn remove_prompt_tag(conn: &Connection, id: &str, tag_id: i64) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "DELETE FROM prompt_tag_relations WHERE prompt_id = ?1 AND tag_id = ?2",
        rusqlite::params![id, tag_id],
    )?;
    // 标签变化视为提示词内容变更，同步 updated_at（与图像侧 remove_image_tag 对称）
    tx.execute(
        "UPDATE prompts SET updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id = ?1",
        rusqlite::params![id],
    )?;
    tx.commit()?;
    Ok(())
}

/// 取消提示词与其一张图像的关联（双向解绑）。
/// 关联变化视为两侧内容变更，同步提示词与图像的 updated_at。
pub fn remove_image(conn: &Connection, prompt_id: &str, image_id: &str) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "DELETE FROM prompt_image_relations WHERE prompt_id = ?1 AND image_id = ?2",
        rusqlite::params![prompt_id, image_id],
    )?;
    tx.execute(
        "UPDATE prompts SET updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id = ?1",
        rusqlite::params![prompt_id],
    )?;
    tx.execute(
        "UPDATE images SET updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id = ?1",
        rusqlite::params![image_id],
    )?;
    tx.commit()?;
    Ok(())
}

fn row_to_prompt(row: &rusqlite::Row) -> Result<Prompt> {
    Ok(Prompt {
        id: row.get(0)?,
        title: row.get(1)?,
        content: row.get(2)?,
        content_translate: row.get(3)?,
        created_at: row.get(4)?,
        updated_at: row.get(5)?,
        is_deleted: row.get::<_, i64>(6)? != 0,
        deleted_at: row.get(7)?,
        is_favorite: row.get::<_, i64>(8)? != 0,
        is_safe: row.get::<_, i64>(9)? != 0,
        note: row.get(10)?,
    })
}

#[cfg(test)]
#[path = "prompt_service.test.rs"]
mod tests;
