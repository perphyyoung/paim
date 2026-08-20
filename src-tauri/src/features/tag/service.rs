//! 标签领域服务：承载标签的业务规则（唯一性校验、列表、删除）。
//! 领域层不感知 Tauri / 持久化类型，通过注入的连接访问数据。

use serde::Serialize;

#[derive(Debug, Serialize, Clone)]
pub struct Tag {
    pub id: i64,
    pub name: String,
}

use rusqlite::{Connection, Result};

fn validate_name(name: &str) -> Result<String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(rusqlite::Error::InvalidParameterName(
            "tag name must not be empty".to_string(),
        ));
    }
    Ok(trimmed.to_string())
}

/// 唯一插入：重名返回已存在的记录。
pub fn create(conn: &Connection, name: &str) -> Result<Tag> {
    let name = validate_name(name)?;
    conn.execute(
        "INSERT INTO tags(name) VALUES (?1) ON CONFLICT(name) DO NOTHING",
        rusqlite::params![name],
    )?;
    let id = conn.query_row(
        "SELECT id FROM tags WHERE name = ?1",
        rusqlite::params![name],
        |r| r.get(0),
    )?;
    Ok(Tag { id, name })
}

pub fn list(conn: &Connection) -> Result<Vec<Tag>> {
    let mut stmt = conn.prepare("SELECT id, name FROM tags ORDER BY name")?;
    let rows = stmt.query_map([], |r| Ok(Tag { id: r.get(0)?, name: r.get(1)? }))?;
    rows.collect()
}

pub fn remove(conn: &Connection, id: i64) -> Result<()> {
    conn.execute("DELETE FROM tags WHERE id = ?1", rusqlite::params![id])?;
    Ok(())
}