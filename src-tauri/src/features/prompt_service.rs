//! 提示词领域服务：承载提示词的业务规则。
//! 领域层不感知 Tauri，通过注入的事务获取连接访问数据。
//! 表结构与字段名与 prompt-manager 一致。

use rusqlite::{Connection, OptionalExtension, Result};

use serde::Serialize;

#[derive(Debug, Serialize, Clone)]
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
            "prompt content must not be empty".to_string(),
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
    let id = crate::db::gen_id(crate::db::PROMPT_ID_PREFIX);
    // 与 pm 一致：未提供标题时，用提示词 id 作为标题
    if title.is_empty() {
        title = id.clone();
    }

    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "INSERT INTO prompts(id, title, content) VALUES (?1, ?2, ?3)",
        rusqlite::params![id, title, content],
    )?;
    let prompt = get_by_id(&tx, &id)?.expect("inserted prompt must exist");
    tx.commit()?;
    Ok(prompt)
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

pub fn update_title(conn: &Connection, id: &str, title: Option<String>) -> Result<Prompt> {
    let title = title
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .unwrap_or_default();
    let tx = conn.unchecked_transaction()?;
    let changed = tx.execute(
        "UPDATE prompts SET title = ?1, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id = ?2",
        rusqlite::params![title, id],
    )?;
    if changed == 0 {
        return Err(rusqlite::Error::QueryReturnedNoRows);
    }
    let prompt = get_by_id(&tx, id)?.expect("updated prompt must exist");
    tx.commit()?;
    Ok(prompt)
}

/// 软删除：标记为已删除（与图像回收站机制一致）。
pub fn remove(conn: &Connection, id: &str) -> Result<()> {
    conn.execute(
        "UPDATE prompts SET is_deleted = 1, deleted_at = strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id = ?1",
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

/// 恢复软删除的提示词。
pub fn restore(conn: &Connection, id: &str) -> Result<Option<Prompt>> {
    conn.execute(
        "UPDATE prompts SET is_deleted = 0, deleted_at = NULL WHERE id = ?1",
        rusqlite::params![id],
    )?;
    get_by_id(conn, id)
}

/// 彻底删除提示词（从数据库中移除）。
pub fn purge(conn: &Connection, id: &str) -> Result<()> {
    conn.execute("DELETE FROM prompts WHERE id = ?1", rusqlite::params![id])?;
    Ok(())
}

/// 恢复全部回收站提示词，返回恢复数量。
pub fn restore_all(conn: &Connection) -> Result<usize> {
    conn.execute(
        "UPDATE prompts SET is_deleted = 0, deleted_at = NULL WHERE is_deleted = 1",
        [],
    )
}

/// 清空回收站提示词（关联关系随外键级联删除），返回清理数量。
pub fn empty_trash(conn: &Connection) -> Result<usize> {
    conn.execute("DELETE FROM prompts WHERE is_deleted = 1", [])
}

/// 更新提示词详情字段（标题/内容/翻译/备注/收藏/安全）。仅更新传入 Some 的值；
/// 标题与内容非空才更新；翻译与备注允许清空；收藏/安全按布尔更新。
pub fn update_detail(
    conn: &Connection,
    id: &str,
    title: Option<String>,
    content: Option<String>,
    content_translate: Option<String>,
    note: Option<String>,
    is_favorite: Option<bool>,
    is_safe: Option<bool>,
) -> Result<Option<Prompt>> {
    let tx = conn.unchecked_transaction()?;
    let mut changed = false;
    if let Some(v) = title {
        let v = v.trim().to_string();
        if !v.is_empty() {
            tx.execute(
                "UPDATE prompts SET title = ?1 WHERE id = ?2",
                rusqlite::params![v, id],
            )?;
            changed = true;
        }
    }
    if let Some(v) = content {
        let v = v.trim().to_string();
        if !v.is_empty() {
            tx.execute(
                "UPDATE prompts SET content = ?1 WHERE id = ?2",
                rusqlite::params![v, id],
            )?;
            changed = true;
        }
    }
    if let Some(v) = content_translate {
        tx.execute(
            "UPDATE prompts SET content_translate = ?1 WHERE id = ?2",
            rusqlite::params![v, id],
        )?;
        changed = true;
    }
    if let Some(v) = note {
        tx.execute(
            "UPDATE prompts SET note = ?1 WHERE id = ?2",
            rusqlite::params![v, id],
        )?;
        changed = true;
    }
    if let Some(v) = is_favorite {
        tx.execute(
            "UPDATE prompts SET is_favorite = ?1 WHERE id = ?2",
            rusqlite::params![v as i64, id],
        )?;
        changed = true;
    }
    if let Some(v) = is_safe {
        tx.execute(
            "UPDATE prompts SET is_safe = ?1 WHERE id = ?2",
            rusqlite::params![v as i64, id],
        )?;
        changed = true;
    }
    if changed {
        tx.execute(
            "UPDATE prompts SET updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id = ?1",
            rusqlite::params![id],
        )?;
    }
    tx.commit()?;
    get_by_id(conn, id)
}

/// 提示词关联的（未删除）图像及其标签，供详情页图像网格展示。
#[derive(Debug, serde::Serialize, Clone)]
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
    let data_dir = crate::db::data_dir(app);
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
        let mut tags = Vec::new();
        {
            let mut ts = conn.prepare(
                "SELECT pt.name
                 FROM image_tag_relations itr
                 JOIN image_tags pt ON pt.id = itr.tag_id
                 WHERE itr.image_id = ?1
                 ORDER BY pt.name",
            )?;
            let rows = ts.query_map(rusqlite::params![id], |r| r.get::<_, String>(0))?;
            for t in rows {
                tags.push(t?);
            }
        }
        out.push(RelatedImage {
            id,
            file_name,
            src: src_rel
                .map(|rel| data_dir.join(&rel).to_string_lossy().into_owned())
                .unwrap_or_default(),
            tags,
        });
    }
    Ok(out)
}

/// 为单个提示词添加一个标签（不存在则创建，关联存在则忽略），返回关联的标签。
pub fn add_prompt_tag(conn: &Connection, id: &str, name: &str) -> Result<Vec<(i64, String)>> {
    let tx = conn.unchecked_transaction()?;
    let tag_id = get_or_create_prompt_tag(&tx, name)?;
    let _ = tx.execute(
        "INSERT OR IGNORE INTO prompt_tag_relations(prompt_id, tag_id) VALUES (?1, ?2)",
        rusqlite::params![id, tag_id],
    );
    tx.commit()?;
    Ok(vec![(tag_id, name.to_string())])
}

/// 为多个提示词批量添加同一个标签（单事务）：一次为多个项目打同一个标签。
pub fn batch_add_prompt_tag(conn: &Connection, ids: &[&str], name: &str) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    let tag_id = get_or_create_prompt_tag(&tx, name)?;
    for id in ids {
        let _ = tx.execute(
            "INSERT OR IGNORE INTO prompt_tag_relations(prompt_id, tag_id) VALUES (?1, ?2)",
            rusqlite::params![id, tag_id],
        );
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
pub fn remove_tag(conn: &Connection, id: &str, tag_id: i64) -> Result<()> {
    conn.execute(
        "DELETE FROM prompt_tag_relations WHERE prompt_id = ?1 AND tag_id = ?2",
        rusqlite::params![id, tag_id],
    )?;
    Ok(())
}

/// 取消提示词与其一张图像的关联。
pub fn remove_image(conn: &Connection, prompt_id: &str, image_id: &str) -> Result<()> {
    conn.execute(
        "DELETE FROM prompt_image_relations WHERE prompt_id = ?1 AND image_id = ?2",
        rusqlite::params![prompt_id, image_id],
    )?;
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
