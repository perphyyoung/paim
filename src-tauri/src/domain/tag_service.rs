//! 标签查询与标签关联：按域（图像/提示词）复用同一套逻辑。
//!
//! 取代原先散落在 image_service / prompt_service 中的两套实现：
//! 全量标签数据、{实体id: [标签名]} 映射、单条目标签列表、增删标签。
//! 表名与列名由 TagDomain 决定（白名单映射，非外部输入，无注入风险）。

use crate::domain::tag_manager::{self, TagData, TagDomain, TagLite};
use crate::infra::error::AppError;
use rusqlite::{Connection, OptionalExtension};
use std::collections::HashMap;

/// 域内全部标签数据（标签组 + 带未删除计数的标签）：筛选区与标签管理页共用。
pub fn load_tag_data(conn: &Connection, domain: TagDomain) -> rusqlite::Result<TagData> {
    crate::domain::tag_manager::load_tag_data(conn, domain)
}

/// 未删除实体到其标签名的映射：{itemId: [tagName,...]}，供列表内存过滤与卡片标签行。
pub fn load_tags_map(
    conn: &Connection,
    domain: TagDomain,
) -> rusqlite::Result<HashMap<String, Vec<String>>> {
    let owner = domain.owner_column();
    let sql = format!(
        "SELECT it.id, t.name
         FROM {items} it
         JOIN {rel} r ON r.{owner} = it.id
         JOIN {tags} t ON t.id = r.tag_id
         WHERE it.is_deleted = 0
         ORDER BY t.name",
        items = domain.items_table(),
        rel = domain.relations_table(),
        tags = domain.tags_table(),
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
    let mut map: HashMap<String, Vec<String>> = HashMap::new();
    for row in rows {
        let (item_id, name) = row?;
        map.entry(item_id).or_default().push(name);
    }
    Ok(map)
}

/// 单个实体的标签列表（按名称升序）。
pub fn load_item_tags(
    conn: &Connection,
    domain: TagDomain,
    item_id: &str,
) -> rusqlite::Result<Vec<TagLite>> {
    let owner = domain.owner_column();
    let sql = format!(
        "SELECT t.id, t.name
         FROM {rel} r
         JOIN {tags} t ON t.id = r.tag_id
         WHERE r.{owner} = ?1
         ORDER BY t.name",
        rel = domain.relations_table(),
        tags = domain.tags_table(),
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(rusqlite::params![item_id], |r| {
        Ok(TagLite {
            id: r.get(0)?,
            name: r.get(1)?,
        })
    })?;
    let mut tags = Vec::new();
    for row in rows {
        tags.push(row?);
    }
    Ok(tags)
}

/// 为单个实体添加一个标签（标签不存在则创建，关联已存在则忽略），并刷新实体 updated_at。
pub fn add_tag(
    conn: &Connection,
    domain: TagDomain,
    item_id: &str,
    name: &str,
) -> std::result::Result<Vec<TagLite>, AppError> {
    let name = name.trim();
    let tx = conn.unchecked_transaction()?;
    ensure_item_exists(&tx, domain, item_id)?;
    let tag_id = get_or_create_tag(&tx, domain, name)?;
    insert_relation(&tx, domain, item_id, tag_id)?;
    touch_updated_at(&tx, domain, item_id)?;
    tx.commit()?;
    Ok(vec![TagLite {
        id: tag_id,
        name: name.to_string(),
    }])
}

/// 为多个实体批量添加同一个标签（单事务），并逐个刷新 updated_at。
pub fn batch_add_tag(
    conn: &Connection,
    domain: TagDomain,
    ids: &[&str],
    name: &str,
) -> std::result::Result<(), AppError> {
    let name = name.trim();
    let tx = conn.unchecked_transaction()?;
    for id in ids {
        ensure_item_exists(&tx, domain, id)?;
    }
    let tag_id = get_or_create_tag(&tx, domain, name)?;
    for id in ids {
        insert_relation(&tx, domain, id, tag_id)?;
        touch_updated_at(&tx, domain, id)?;
    }
    tx.commit()?;
    Ok(())
}

/// 移除实体的一个标签关联，并刷新实体 updated_at。
pub fn remove_tag(
    conn: &Connection,
    domain: TagDomain,
    item_id: &str,
    tag_id: i64,
) -> std::result::Result<(), AppError> {
    let owner = domain.owner_column();
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        &format!(
            "DELETE FROM {} WHERE {owner} = ?1 AND tag_id = ?2",
            domain.relations_table()
        ),
        rusqlite::params![item_id, tag_id],
    )?;
    touch_updated_at(&tx, domain, item_id)?;
    tx.commit()?;
    Ok(())
}

/// 校验实体存在，不存在则报错（避免静默创建孤立标签关联）。
fn ensure_item_exists(
    tx: &rusqlite::Transaction,
    domain: TagDomain,
    item_id: &str,
) -> std::result::Result<(), AppError> {
    let found = tx
        .query_row(
            &format!("SELECT 1 FROM {} WHERE id = ?1", domain.items_table()),
            rusqlite::params![item_id],
            |_| Ok(()),
        )
        .optional()?;
    if found.is_none() {
        return Err(AppError::Message(format!(
            "{} {item_id} 不存在",
            domain.label()
        )));
    }
    Ok(())
}

/// 获取标签 id，不存在则创建（供单条与批量场景复用）；新建前校验域内标签数上限。
fn get_or_create_tag(
    tx: &rusqlite::Transaction,
    domain: TagDomain,
    name: &str,
) -> std::result::Result<i64, AppError> {
    match tx
        .query_row(
            &format!("SELECT id FROM {} WHERE name = ?1", domain.tags_table()),
            rusqlite::params![name],
            |r| r.get(0),
        )
        .optional()?
    {
        Some(id) => Ok(id),
        None => {
            tag_manager::ensure_tag_capacity(tx, domain).map_err(AppError::Message)?;
            tx.execute(
                &format!("INSERT INTO {}(name) VALUES (?1)", domain.tags_table()),
                rusqlite::params![name],
            )?;
            Ok(tx.last_insert_rowid())
        }
    }
}

fn insert_relation(
    tx: &rusqlite::Transaction,
    domain: TagDomain,
    item_id: &str,
    tag_id: i64,
) -> rusqlite::Result<()> {
    let owner = domain.owner_column();
    tx.execute(
        &format!(
            "INSERT OR IGNORE INTO {}({owner}, tag_id) VALUES (?1, ?2)",
            domain.relations_table()
        ),
        rusqlite::params![item_id, tag_id],
    )?;
    Ok(())
}

/// 标签变化视为实体内容变更，同步 updated_at（列表按时间排序时生效）。
fn touch_updated_at(
    tx: &rusqlite::Transaction,
    domain: TagDomain,
    item_id: &str,
) -> rusqlite::Result<()> {
    tx.execute(
        &format!(
            "UPDATE {} SET updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id = ?1",
            domain.items_table()
        ),
        rusqlite::params![item_id],
    )?;
    Ok(())
}

#[cfg(test)]
#[path = "tag_service.test.rs"]
mod tests;
