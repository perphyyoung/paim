//! 提示词领域服务：承载提示词的业务规则。
//! 领域层不感知 Tauri，通过注入的事务获取连接访问数据。

use rusqlite::{Connection, Result};

use serde::Serialize;

#[derive(Debug, Serialize, Clone)]
pub struct Prompt {
    pub id: i64,
    pub content: String,
    pub title: Option<String>,
    pub created_at: String,
    pub updated_at: String,
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
    let title = title.map(|t| t.trim().to_string()).filter(|t| !t.is_empty());

    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "INSERT INTO prompts(content, title) VALUES (?1, ?2)",
        rusqlite::params![content, title],
    )?;
    let id = tx.last_insert_rowid();
    let prompt = get_by_id(&tx, id)?.expect("inserted prompt must exist");
    tx.commit()?;
    Ok(prompt)
}

pub fn get_by_id(conn: &Connection, id: i64) -> Result<Option<Prompt>> {
    let mut stmt = conn.prepare(
        "SELECT id, content, title, created_at, updated_at FROM prompts WHERE id = ?1",
    )?;
    let mut rows = stmt.query_map(rusqlite::params![id], row_to_prompt)?;
    rows.next().transpose()
}

pub fn list(conn: &Connection) -> Result<Vec<Prompt>> {
    let mut stmt = conn.prepare(
        "SELECT id, content, title, created_at, updated_at FROM prompts ORDER BY updated_at DESC",
    )?;
    let rows = stmt.query_map([], row_to_prompt)?;
    rows.collect()
}

pub fn update_title(conn: &Connection, id: i64, title: Option<String>) -> Result<Prompt> {
    let title = title.map(|t| t.trim().to_string()).filter(|t| !t.is_empty());
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

pub fn remove(conn: &Connection, id: i64) -> Result<()> {
    conn.execute("DELETE FROM prompts WHERE id = ?1", rusqlite::params![id])?;
    Ok(())
}

fn row_to_prompt(row: &rusqlite::Row) -> Result<Prompt> {
    Ok(Prompt {
        id: row.get(0)?,
        content: row.get(1)?,
        title: row.get(2)?,
        created_at: row.get(3)?,
        updated_at: row.get(4)?,
    })
}