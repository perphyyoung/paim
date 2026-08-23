//! 提示词领域服务：承载提示词的业务规则。
//! 领域层不感知 Tauri，通过注入的事务获取连接访问数据。
//! 表结构与字段名与 prompt-manager 一致。

use rusqlite::{Connection, Result};

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