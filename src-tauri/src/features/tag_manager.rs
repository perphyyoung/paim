//! 通用标签管理：标签组 + 标签的 CRUD，按域（图像/提示词）复用同一套逻辑。
//! 表名前缀由 TagDomain 决定（白名单映射，非外部输入，无注入风险），
//! 供图像标签管理与提示词标签管理共用。

use rusqlite::{Connection, OptionalExtension};
use serde::Serialize;

/// 标签所属域：决定表名前缀（image_/prompt_）与文案。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TagDomain {
    Image,
    Prompt,
}

/// 名称重名校验的目标对象类型（决定查标签表还是组表）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TagNameKind {
    Tag,
    Group,
}

/// 校验域内标签/组名唯一（exclude_id 用于更新时排除自身）；已存在返回中文错误文本。
pub fn ensure_name_not_dup(
    conn: &Connection,
    domain: TagDomain,
    kind: TagNameKind,
    name: &str,
    exclude_id: Option<i64>,
) -> Result<(), String> {
    let table = match kind {
        TagNameKind::Tag => domain.tags_table(),
        TagNameKind::Group => domain.groups_table(),
    };
    let dup = match exclude_id {
        Some(id) => conn
            .query_row(
                &format!("SELECT 1 FROM {table} WHERE name = ?1 AND id != ?2 LIMIT 1"),
                rusqlite::params![name.trim(), id],
                |_| Ok(()),
            )
            .optional(),
        None => conn
            .query_row(
                &format!("SELECT 1 FROM {table} WHERE name = ?1 LIMIT 1"),
                rusqlite::params![name.trim()],
                |_| Ok(()),
            )
            .optional(),
    }
    .map_err(|e| e.to_string())?
    .is_some();
    if dup {
        let what = match kind {
            TagNameKind::Tag => "标签",
            TagNameKind::Group => "标签组",
        };
        Err(format!("同名{what}已存在"))
    } else {
        Ok(())
    }
}

impl TagDomain {
    fn tags_table(self) -> &'static str {
        match self {
            TagDomain::Image => "image_tags",
            TagDomain::Prompt => "prompt_tags",
        }
    }
    fn groups_table(self) -> &'static str {
        match self {
            TagDomain::Image => "image_tag_groups",
            TagDomain::Prompt => "prompt_tag_groups",
        }
    }
    fn relations_table(self) -> &'static str {
        match self {
            TagDomain::Image => "image_tag_relations",
            TagDomain::Prompt => "prompt_tag_relations",
        }
    }
    /// 面向用户的对象名，用于文案（如「图像标签管理」）。
    pub fn label(self) -> &'static str {
        match self {
            TagDomain::Image => "图像",
            TagDomain::Prompt => "提示词",
        }
    }
}

/// 标签管理页中的标签（含所属组与关联对象数）。
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

pub fn load_manager_data(conn: &Connection, domain: TagDomain) -> rusqlite::Result<TagManagerData> {
    let mut groups = Vec::new();
    {
        let sql = format!(
            "SELECT id, name, sort_order FROM {} ORDER BY sort_order, name",
            domain.groups_table()
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map([], |r| {
            Ok(TagGroup {
                id: r.get(0)?,
                name: r.get(1)?,
                sort_order: r.get(2)?,
            })
        })?;
        for row in rows {
            groups.push(row?);
        }
    }
    let mut tags = Vec::new();
    {
        let sql = format!(
            "SELECT t.id, t.name, t.group_id, COUNT(r.tag_id) AS cnt
             FROM {} t
             LEFT JOIN {} r ON r.tag_id = t.id
             GROUP BY t.id
             ORDER BY t.name",
            domain.tags_table(),
            domain.relations_table()
        );
        let mut stmt = conn.prepare(&sql)?;
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

/// 新建标签组，返回新组。未指定排序时追加到末尾。
pub fn create_group(
    conn: &Connection,
    domain: TagDomain,
    name: &str,
    sort_order: Option<i64>,
) -> rusqlite::Result<TagGroup> {
    let name = name.trim();
    let sort_order = match sort_order {
        Some(v) => v,
        None => {
            let sql = format!(
                "SELECT COALESCE(MAX(sort_order) + 1, 0) FROM {}",
                domain.groups_table()
            );
            conn.query_row(&sql, [], |r| r.get(0))?
        }
    };
    let sql = format!(
        "INSERT INTO {}(name, sort_order) VALUES (?1, ?2)",
        domain.groups_table()
    );
    conn.execute(&sql, rusqlite::params![name, sort_order])?;
    let id = conn.last_insert_rowid();
    Ok(TagGroup {
        id,
        name: name.to_string(),
        sort_order,
    })
}

/// 编辑标签组：更新名称与排序数值。
pub fn update_group(
    conn: &Connection,
    domain: TagDomain,
    id: i64,
    name: &str,
    sort_order: Option<i64>,
) -> rusqlite::Result<()> {
    let name = name.trim();
    let sql = format!(
        "UPDATE {} SET name = ?1, sort_order = COALESCE(?2, sort_order), updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id = ?3",
        domain.groups_table()
    );
    conn.execute(&sql, rusqlite::params![name, sort_order, id])?;
    Ok(())
}

/// 删除标签组（组内标签交由外键 ON DELETE SET NULL 变为未分组）。
pub fn delete_group(conn: &Connection, domain: TagDomain, id: i64) -> rusqlite::Result<()> {
    let sql = format!("DELETE FROM {} WHERE id = ?1", domain.groups_table());
    conn.execute(&sql, rusqlite::params![id])?;
    Ok(())
}

/// 新建标签（可指定所属组），返回新标签。
pub fn create_tag(
    conn: &Connection,
    domain: TagDomain,
    name: &str,
    group_id: Option<i64>,
) -> rusqlite::Result<TagItem> {
    let name = name.trim();
    let sql = format!(
        "INSERT INTO {}(name, group_id) VALUES (?1, ?2)",
        domain.tags_table()
    );
    conn.execute(&sql, rusqlite::params![name, group_id])?;
    let id = conn.last_insert_rowid();
    Ok(TagItem {
        id,
        name: name.to_string(),
        group_id,
        count: 0,
    })
}

/// 重命名标签。
pub fn rename_tag(
    conn: &Connection,
    domain: TagDomain,
    id: i64,
    name: &str,
) -> rusqlite::Result<()> {
    let name = name.trim();
    let sql = format!(
        "UPDATE {} SET name = ?1, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id = ?2",
        domain.tags_table()
    );
    conn.execute(&sql, rusqlite::params![name, id])?;
    Ok(())
}

/// 删除标签（关联关系由外键 CASCADE 一并清除）。
pub fn delete_tag(conn: &Connection, domain: TagDomain, id: i64) -> rusqlite::Result<()> {
    let sql = format!("DELETE FROM {} WHERE id = ?1", domain.tags_table());
    conn.execute(&sql, rusqlite::params![id])?;
    Ok(())
}

/// 移动标签到指定组（group_id 为 null 表示未分组）。
pub fn move_tag(
    conn: &Connection,
    domain: TagDomain,
    id: i64,
    group_id: Option<i64>,
) -> rusqlite::Result<()> {
    let sql = format!(
        "UPDATE {} SET group_id = ?1, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id = ?2",
        domain.tags_table()
    );
    conn.execute(&sql, rusqlite::params![group_id, id])?;
    Ok(())
}

/// 将标签组固定到首位（sort_order 设为当前最小值 - 1）。
pub fn pin_group_to_top(conn: &Connection, domain: TagDomain, id: i64) -> rusqlite::Result<()> {
    let first_sql = format!(
        "SELECT COALESCE(MIN(sort_order), 0) FROM {}",
        domain.groups_table()
    );
    let first: i64 = conn.query_row(&first_sql, [], |r| r.get(0))?;
    let sql = format!(
        "UPDATE {} SET sort_order = ?1, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id = ?2",
        domain.groups_table()
    );
    conn.execute(&sql, rusqlite::params![first - 1, id])?;
    Ok(())
}
