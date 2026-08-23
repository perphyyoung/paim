//! 图像标签管理：标签组 + 标签的 CRUD。
//! 数据源为 image_tags / image_tag_groups 表，供「图像标签管理」页使用，
//! 独立于此域下的图像导入/查询逻辑。

use rusqlite::Connection;
use serde::Serialize;
use tauri::State;

/// 标签管理页中的标签（含所属组与关联图片数）。
#[derive(Debug, Serialize, Clone)]
pub struct TagItem {
    pub id: i64,
    pub name: String,
    pub group_id: Option<i64>,
    pub count: i64,
}

/// 标签管理页中的标签组（含排序序号，首位组即 sort_order 最小者）。
#[derive(Debug, Serialize, Clone)]
pub struct TagGroup {
    pub id: i64,
    pub name: String,
    pub sort_order: i64,
}

/// 标签管理页数据：全部标签组 + 全部标签（含计数），供前端按需分组/排序。
#[derive(Debug, Serialize, Clone)]
pub struct TagManagerData {
    pub groups: Vec<TagGroup>,
    pub tags: Vec<TagItem>,
}

fn load_tag_manager_data(conn: &Connection) -> rusqlite::Result<TagManagerData> {
    let mut groups = Vec::new();
    {
        let mut stmt = conn.prepare("SELECT id, name, sort_order FROM image_tag_groups ORDER BY sort_order, name")?;
        let rows = stmt.query_map([], |r| {
            Ok(TagGroup { id: r.get(0)?, name: r.get(1)?, sort_order: r.get(2)? })
        })?;
        for row in rows {
            groups.push(row?);
        }
    }
    let mut tags = Vec::new();
    {
        let mut stmt = conn.prepare(
            "SELECT t.id, t.name, t.group_id, COUNT(r.tag_id) AS cnt
             FROM image_tags t
             LEFT JOIN image_tag_relations r ON r.tag_id = t.id
             GROUP BY t.id
             ORDER BY t.name",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(TagItem {
                id: r.get(0)?,
                name: r.get(1)?,
                group_id: r.get(2)?,
                count: r.get(3)?,
            })
        })?;
        for row in rows {
            tags.push(row?);
        }
    }
    Ok(TagManagerData { groups, tags })
}

/// 返回标签管理页所需数据（标签组 + 带计数的标签）。
#[tauri::command]
pub fn list_image_tag_groups(db: State<crate::db::BkDb>) -> Result<TagManagerData, String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    load_tag_manager_data(&conn).map_err(|e| e.to_string())
}

/// 新建标签组，返回新组。
#[tauri::command]
pub fn create_image_tag_group(
    db: State<crate::db::BkDb>,
    name: String,
    sort_order: Option<i64>,
) -> Result<TagGroup, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("组名不能为空".to_string());
    }
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    // 未指定排序时追加到末尾
    let sort_order = match sort_order {
        Some(v) => v,
        None => conn
            .query_row(
                "SELECT COALESCE(MAX(sort_order) + 1, 0) FROM image_tag_groups",
                [],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?,
    };
    conn.execute(
        "INSERT INTO image_tag_groups(name, sort_order) VALUES (?1, ?2)",
        rusqlite::params![name, sort_order],
    )
    .map_err(|e| e.to_string())?;
    let id = conn.last_insert_rowid();
    Ok(TagGroup {
        id,
        name: name.to_string(),
        sort_order,
    })
}

/// 编辑标签组：更新名称与排序数值。
#[tauri::command]
pub fn update_image_tag_group(
    db: State<crate::db::BkDb>,
    id: i64,
    name: String,
    sort_order: Option<i64>,
) -> Result<(), String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("组名不能为空".to_string());
    }
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    conn.execute(
        "UPDATE image_tag_groups SET name = ?1, sort_order = COALESCE(?2, sort_order), updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id = ?3",
        rusqlite::params![name, sort_order, id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// 删除标签组（组内标签交由外键 ON DELETE SET NULL 变为未分组）。
#[tauri::command]
pub fn delete_image_tag_group(db: State<crate::db::BkDb>, id: i64) -> Result<(), String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM image_tag_groups WHERE id = ?1", rusqlite::params![id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// 新建标签（可指定所属组），返回新标签。
#[tauri::command]
pub fn create_image_tag(
    db: State<crate::db::BkDb>,
    name: String,
    group_id: Option<i64>,
) -> Result<TagItem, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("标签名不能为空".to_string());
    }
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO image_tags(name, group_id) VALUES (?1, ?2)",
        rusqlite::params![name, group_id],
    )
    .map_err(|e| e.to_string())?;
    let id = conn.last_insert_rowid();
    Ok(TagItem { id, name: name.to_string(), group_id, count: 0 })
}

/// 重命名标签。
#[tauri::command]
pub fn rename_image_tag(db: State<crate::db::BkDb>, id: i64, name: String) -> Result<(), String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("标签名不能为空".to_string());
    }
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    conn.execute(
        "UPDATE image_tags SET name = ?1, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id = ?2",
        rusqlite::params![name, id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// 删除标签（关联关系由外键 CASCADE 一并清除）。
#[tauri::command]
pub fn delete_image_tag(db: State<crate::db::BkDb>, id: i64) -> Result<(), String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM image_tags WHERE id = ?1", rusqlite::params![id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// 移动标签到指定组（group_id 为 null 表示未分组）。
#[tauri::command]
pub fn move_tag_to_group(
    db: State<crate::db::BkDb>,
    id: i64,
    group_id: Option<i64>,
) -> Result<(), String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    conn.execute(
        "UPDATE image_tags SET group_id = ?1, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id = ?2",
        rusqlite::params![group_id, id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// 将标签组固定到首位（sort_order 设为当前最小值 - 1）。
#[tauri::command]
pub fn pin_image_tag_group_to_top(db: State<crate::db::BkDb>, id: i64) -> Result<(), String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    let first = conn
        .query_row(
            "SELECT COALESCE(MIN(sort_order), 0) FROM image_tag_groups",
            [],
            |r| r.get::<_, i64>(0),
        )
        .map_err(|e| e.to_string())?;
    conn.execute(
        "UPDATE image_tag_groups SET sort_order = ?1, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id = ?2",
        rusqlite::params![first - 1, id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}