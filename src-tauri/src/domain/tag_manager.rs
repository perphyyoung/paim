//! 通用标签管理：标签组 + 标签的 CRUD，按域（图像/提示词）复用同一套逻辑。
//! 表名前缀由 TagDomain 决定（白名单映射，非外部输入，无注入风险），
//! 供图像标签管理与提示词标签管理共用。

use rusqlite::{Connection, OptionalExtension, Result as RusqliteResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 标签所属域：决定表名前缀（image_/prompt_）与文案。
/// 作为命令参数下发到前端，serde/specta 用小写字符串（"image" / "prompt"）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "lowercase")]
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

/// 单个域的标签数量上限。标签按**小规模**设计（每域 500 条封顶，与图像/提示词条数无关），
/// 达到上限即拒绝新增，前端直接提示；因此筛选区与管理弹窗都按全量渲染，不做分批/上限渲染。
pub const MAX_TAGS_PER_DOMAIN: i64 = 500;

/// 校验域内标签数未达上限（新增标签前调用）；已达上限返回中文错误文本。
pub fn ensure_tag_capacity(conn: &Connection, domain: TagDomain) -> Result<(), String> {
    let count: i64 = conn
        .query_row(
            &format!("SELECT COUNT(*) FROM {}", domain.tags_table()),
            [],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    if count >= MAX_TAGS_PER_DOMAIN {
        Err(format!(
            "{}标签已达上限（{MAX_TAGS_PER_DOMAIN} 个），无法新增",
            domain.label()
        ))
    } else {
        Ok(())
    }
}

/// 首位组（sort_order 最小的标签组）的标签数上限：首位组在筛选区收起后仍参与布局，
/// 过多会把卡片区挤出视口（约束说明见根目录 readme.md「标签按小规模设计」）。
pub const MAX_TAGS_IN_TOP_GROUP: i64 = 100;

/// 当前首位组 id（无任何组时为 None）。
fn top_group_id(conn: &Connection, domain: TagDomain) -> RusqliteResult<Option<i64>> {
    conn.query_row(
        &format!(
            "SELECT id FROM {} ORDER BY sort_order ASC, id ASC LIMIT 1",
            domain.groups_table()
        ),
        [],
        |r| r.get(0),
    )
    .optional()
}

/// 组内标签数（exclude_tag 用于移动场景排除被移动标签自身，避免同名组内移动误判）。
fn group_tag_count(
    conn: &Connection,
    domain: TagDomain,
    group_id: i64,
    exclude_tag: Option<i64>,
) -> RusqliteResult<i64> {
    match exclude_tag {
        Some(id) => conn.query_row(
            &format!(
                "SELECT COUNT(*) FROM {} WHERE group_id = ?1 AND id != ?2",
                domain.tags_table()
            ),
            rusqlite::params![group_id, id],
            |r| r.get(0),
        ),
        None => conn.query_row(
            &format!(
                "SELECT COUNT(*) FROM {} WHERE group_id = ?1",
                domain.tags_table()
            ),
            rusqlite::params![group_id],
            |r| r.get(0),
        ),
    }
}

/// 校验标签加入 group_id 后首位组不超上限（仅当目标组是首位组时约束）；
/// 已达上限返回中文错误文本。
pub fn ensure_top_group_capacity(
    conn: &Connection,
    domain: TagDomain,
    group_id: i64,
    exclude_tag: Option<i64>,
) -> Result<(), String> {
    if top_group_id(conn, domain).map_err(|e| e.to_string())? != Some(group_id) {
        return Ok(());
    }
    if group_tag_count(conn, domain, group_id, exclude_tag).map_err(|e| e.to_string())?
        >= MAX_TAGS_IN_TOP_GROUP
    {
        Err(format!(
            "首位组最多 {MAX_TAGS_IN_TOP_GROUP} 个标签，无法再加入"
        ))
    } else {
        Ok(())
    }
}

/// 校验 group_id 即将成为首位组（置顶 / 改 sort_order）时不超上限；已是首位组则跳过。
pub fn ensure_group_as_top_capacity(
    conn: &Connection,
    domain: TagDomain,
    group_id: i64,
) -> Result<(), String> {
    if top_group_id(conn, domain).map_err(|e| e.to_string())? == Some(group_id) {
        return Ok(());
    }
    // 加入路径满 100 即拒（>=），置顶路径允许恰好 100（>100 才拒），两处口径均为「首位组最多 100 个」
    if group_tag_count(conn, domain, group_id, None).map_err(|e| e.to_string())?
        > MAX_TAGS_IN_TOP_GROUP
    {
        Err(format!(
            "该组标签超过 {MAX_TAGS_IN_TOP_GROUP} 个，不能固定到首位"
        ))
    } else {
        Ok(())
    }
}

/// 校验修改组排序（update_group 的 sort_order）后首位组不超上限；
/// 仅当新 sort_order 会使该组成为首位组时约束。
pub fn ensure_group_resort_capacity(
    conn: &Connection,
    domain: TagDomain,
    group_id: i64,
    new_sort_order: i64,
) -> Result<(), String> {
    let min_others: i64 = conn
        .query_row(
            &format!(
                "SELECT COALESCE(MIN(sort_order), 0) FROM {} WHERE id != ?1",
                domain.groups_table()
            ),
            rusqlite::params![group_id],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    if new_sort_order > min_others {
        return Ok(());
    }
    ensure_group_as_top_capacity(conn, domain, group_id)
}

impl TagDomain {
    pub fn tags_table(self) -> &'static str {
        match self {
            TagDomain::Image => "image_tags",
            TagDomain::Prompt => "prompt_tags",
        }
    }
    pub fn groups_table(self) -> &'static str {
        match self {
            TagDomain::Image => "image_tag_groups",
            TagDomain::Prompt => "prompt_tag_groups",
        }
    }
    pub fn relations_table(self) -> &'static str {
        match self {
            TagDomain::Image => "image_tag_relations",
            TagDomain::Prompt => "prompt_tag_relations",
        }
    }
    /// 标签回收站表（删除时写快照，恢复时重建）。
    pub fn trash_table(self) -> &'static str {
        match self {
            TagDomain::Image => "image_tag_trash",
            TagDomain::Prompt => "prompt_tag_trash",
        }
    }
    /// 关联表中指向实体 id 的列名（image_id / prompt_id）。
    pub fn owner_column(self) -> &'static str {
        match self {
            TagDomain::Image => "image_id",
            TagDomain::Prompt => "prompt_id",
        }
    }
    /// 被标签标注的实体表（images / prompts），用于计数与存在性校验。
    pub fn items_table(self) -> &'static str {
        match self {
            TagDomain::Image => "images",
            TagDomain::Prompt => "prompts",
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

/// 批量查询一批实体的标签，按实体 id 分组返回（各组按标签名升序）。
/// 一次 `IN (...)` 覆盖全部 id，供详情类列表回填使用，避免循环内 prepare 的 N+1。
pub fn tags_by_owner(
    conn: &Connection,
    domain: TagDomain,
    ids: &[&str],
) -> RusqliteResult<HashMap<String, Vec<String>>> {
    let mut out: HashMap<String, Vec<String>> = HashMap::new();
    if ids.is_empty() {
        return Ok(out);
    }
    let owner = domain.owner_column();
    let placeholders = std::iter::repeat("?")
        .take(ids.len())
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!(
        "SELECT r.{owner}, t.name
         FROM {rel} r
         JOIN {tags} t ON t.id = r.tag_id
         WHERE r.{owner} IN ({placeholders})
         ORDER BY t.name",
        rel = domain.relations_table(),
        tags = domain.tags_table(),
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(rusqlite::params_from_iter(ids.iter()), |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
    })?;
    for row in rows {
        let (owner_id, name) = row?;
        out.entry(owner_id).or_default().push(name);
    }
    Ok(out)
}

/// 标签精简结构（id + 名称）：单条目标签列表、增删标签的返回统一使用。
#[derive(Debug, Serialize, Clone, specta::Type)]
pub struct TagLite {
    pub id: i64,
    pub name: String,
}

/// 标签（含所属组与关联对象数；count 只统计未删除的实体）。
#[derive(Debug, Serialize, Clone, specta::Type)]
pub struct TagItem {
    pub id: i64,
    pub name: String,
    pub group_id: Option<i64>,
    pub count: i64,
}

/// 标签组（含排序序号，首位组即 sort_order 最小者）。
#[derive(Debug, Serialize, Clone, specta::Type)]
pub struct TagGroup {
    pub id: i64,
    pub name: String,
    pub sort_order: i64,
}

/// 标签数据（标签组 + 全部标签含计数）：标签筛选区与标签管理页共用。
#[derive(Debug, Serialize, Clone, specta::Type)]
pub struct TagData {
    pub groups: Vec<TagGroup>,
    pub tags: Vec<TagItem>,
}

/// 读取域内全部标签数据：标签组（按 sort_order）+ 标签（按名称，含未删除实体计数）。
pub fn load_tag_data(conn: &Connection, domain: TagDomain) -> rusqlite::Result<TagData> {
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
        // count 只统计未删除实体：LEFT JOIN 实体表并带 is_deleted = 0，
        // 回收站内的条目不计入角标（删除后计数随之下降）。
        let sql = format!(
            "SELECT t.id, t.name, t.group_id, COUNT(i.id) AS cnt
             FROM {tags} t
             LEFT JOIN {rel} r ON r.tag_id = t.id
             LEFT JOIN {items} i ON i.id = r.{owner} AND i.is_deleted = 0
             GROUP BY t.id
             ORDER BY t.name",
            tags = domain.tags_table(),
            rel = domain.relations_table(),
            items = domain.items_table(),
            owner = domain.owner_column(),
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
    Ok(TagData { groups, tags })
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

/// 删除标签并在回收站留快照（事务内；调用方负责 commit / rollback）。
/// 采集顺序必须是「先读 owner ids，再 DELETE」——DELETE 会级联清关联。
/// 所有「删除标签」的入口（直接删除 / 合并时的源标签）都必须经这里，避免漏写回收站。
fn trash_tag_in_tx(tx: &rusqlite::Transaction, domain: TagDomain, id: i64) -> Result<(), String> {
    let tags = domain.tags_table();
    let rel = domain.relations_table();
    let owner = domain.owner_column();
    let trash = domain.trash_table();
    let (name, group_id): (String, Option<i64>) = tx
        .query_row(
            &format!("SELECT name, group_id FROM {tags} WHERE id = ?1"),
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => "标签不存在".to_string(),
            _ => e.to_string(),
        })?;
    let owners: Vec<String> = {
        let mut stmt = tx
            .prepare(&format!("SELECT {owner} FROM {rel} WHERE tag_id = ?1"))
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([id], |r| r.get::<_, String>(0))
            .map_err(|e| e.to_string())?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|e| e.to_string())?
    };
    let owner_json = serde_json::to_string(&owners).map_err(|e| e.to_string())?;
    tx.execute(
        &format!(
            "INSERT INTO {trash}(tag_name, group_id, owner_ids, owner_count) VALUES (?1, ?2, ?3, ?4)"
        ),
        rusqlite::params![name, group_id, owner_json, owners.len() as i64],
    )
    .map_err(|e| e.to_string())?;
    tx.execute(&format!("DELETE FROM {tags} WHERE id = ?1"), [id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// 删除标签（进回收站）：写快照后物理删除，恢复时按快照重建。
pub fn delete_tag(conn: &Connection, domain: TagDomain, id: i64) -> Result<(), String> {
    let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
    trash_tag_in_tx(&tx, domain, id)?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(())
}

/// 回收站标签（列表展示：名称 / 关联数 / 删除时间）。
#[derive(Debug, Serialize, Clone, specta::Type)]
pub struct TrashedTag {
    pub id: i64,
    pub name: String,
    pub owner_count: i64,
    pub deleted_at: String,
}

/// 列出回收站标签（最近删除在前）。
pub fn list_trashed(conn: &Connection, domain: TagDomain) -> rusqlite::Result<Vec<TrashedTag>> {
    let sql = format!(
        "SELECT id, tag_name, owner_count, deleted_at FROM {} ORDER BY deleted_at DESC, id DESC",
        domain.trash_table()
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([], |r| {
        Ok(TrashedTag {
            id: r.get(0)?,
            name: r.get(1)?,
            owner_count: r.get(2)?,
            deleted_at: r.get(3)?,
        })
    })?;
    rows.collect()
}

/// 恢复回收站标签：同名活动标签存在则复用并合并关联，否则新建（组不存在时落未分组）；
/// 仅还原仍存在且未删除的实体关联，最后删除回收站记录。
pub fn restore_trashed(conn: &Connection, domain: TagDomain, trash_id: i64) -> Result<(), String> {
    let tags = domain.tags_table();
    let rel = domain.relations_table();
    let owner = domain.owner_column();
    let items = domain.items_table();
    let trash = domain.trash_table();
    let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
    let (name, group_id, owner_json): (String, Option<i64>, String) = tx
        .query_row(
            &format!("SELECT tag_name, group_id, owner_ids FROM {trash} WHERE id = ?1"),
            [trash_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => "回收站标签不存在".to_string(),
            _ => e.to_string(),
        })?;
    let owner_ids: Vec<String> = serde_json::from_str(&owner_json).map_err(|e| e.to_string())?;
    // 组已删则恢复为未分组
    let group_id = match group_id {
        Some(gid) => {
            let exists = tx
                .query_row(
                    &format!("SELECT 1 FROM {} WHERE id = ?1", domain.groups_table()),
                    [gid],
                    |_| Ok(()),
                )
                .optional()
                .map_err(|e| e.to_string())?
                .is_some();
            if exists {
                Some(gid)
            } else {
                None
            }
        }
        None => None,
    };
    let existing: Option<i64> = tx
        .query_row(
            &format!("SELECT id FROM {tags} WHERE name = ?1"),
            [&name],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    let tag_id = match existing {
        Some(id) => id,
        None => {
            ensure_tag_capacity(&tx, domain)?;
            tx.execute(
                &format!("INSERT INTO {tags}(name, group_id) VALUES (?1, ?2)"),
                rusqlite::params![name, group_id],
            )
            .map_err(|e| e.to_string())?;
            tx.last_insert_rowid()
        }
    };
    for owner_id in &owner_ids {
        tx.execute(
            &format!(
                "INSERT OR IGNORE INTO {rel}({owner}, tag_id)
                 SELECT id, ?2 FROM {items} WHERE id = ?1 AND is_deleted = 0"
            ),
            rusqlite::params![owner_id, tag_id],
        )
        .map_err(|e| e.to_string())?;
    }
    tx.execute(&format!("DELETE FROM {trash} WHERE id = ?1"), [trash_id])
        .map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(())
}

/// 彻底删除一条回收站标签。
pub fn purge_trashed(
    conn: &Connection,
    domain: TagDomain,
    trash_id: i64,
) -> rusqlite::Result<usize> {
    conn.execute(
        &format!("DELETE FROM {} WHERE id = ?1", domain.trash_table()),
        [trash_id],
    )
}

/// 恢复全部回收站标签，返回恢复数量。
pub fn restore_all_trashed(conn: &Connection, domain: TagDomain) -> Result<usize, String> {
    let ids: Vec<i64> = {
        let mut stmt = conn
            .prepare(&format!(
                "SELECT id FROM {} ORDER BY id",
                domain.trash_table()
            ))
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |r| r.get(0))
            .map_err(|e| e.to_string())?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|e| e.to_string())?
    };
    let mut restored = 0;
    for id in ids {
        restore_trashed(conn, domain, id)?;
        restored += 1;
    }
    Ok(restored)
}

/// 清空回收站，返回清理数量。
pub fn empty_trash(conn: &Connection, domain: TagDomain) -> rusqlite::Result<usize> {
    conn.execute(&format!("DELETE FROM {}", domain.trash_table()), [])
}

/// 合并两个标签（同域）：源标签的关联迁移到目标后删除源标签。
/// 复合主键 `(owner, tag_id)` 冲突的行由 `INSERT OR IGNORE` 跳过，再按源标签清理，天然去重；
/// 目标标签的名称 / 组保持不变，源的组随标签删除（不迁移）。
pub fn merge_tags(
    conn: &Connection,
    domain: TagDomain,
    source_id: i64,
    target_id: i64,
) -> Result<(), String> {
    if source_id == target_id {
        return Err("不能合并到自身".into());
    }
    let tags = domain.tags_table();
    let rel = domain.relations_table();
    let owner = domain.owner_column();

    let exists = |id: i64| -> Result<bool, String> {
        conn.query_row(&format!("SELECT 1 FROM {tags} WHERE id = ?1"), [id], |_| {
            Ok(())
        })
        .optional()
        .map(|v| v.is_some())
        .map_err(|e| e.to_string())
    };
    if !exists(source_id)? || !exists(target_id)? {
        return Err("标签不存在".into());
    }

    let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
    // 目标已关联的实体发生主键冲突 → 跳过（不重复）；随后清掉源标签的残留关联
    tx.execute(
        &format!(
            "INSERT OR IGNORE INTO {rel}({owner}, tag_id)
             SELECT {owner}, ?2 FROM {rel} WHERE tag_id = ?1"
        ),
        rusqlite::params![source_id, target_id],
    )
    .map_err(|e| e.to_string())?;
    // 源标签的关联此刻仍在：快照采集到的是合并前的完整关联；
    // 随后 DELETE tags 由外键 CASCADE 清掉源关联（不再单独 DELETE rel）。
    trash_tag_in_tx(&tx, domain, source_id)?;
    tx.commit().map_err(|e| e.to_string())?;
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

#[cfg(test)]
#[path = "tag_manager.test.rs"]
mod tests;
