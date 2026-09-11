//! 提示词领域服务：承载提示词的业务规则。
//! 领域层不感知 Tauri，通过注入的事务获取连接访问数据。
//! 表结构与字段名与 prompt-manager 一致。

use crate::domain::list_query::{self, ListQuery};
use crate::domain::tag_manager::{tags_by_owner, TagDomain};
use crate::domain::thumbnail_service::{self, ThumbnailEnsureFixed};
use crate::infra::error::AppError;
use rusqlite::types::Value;
use rusqlite::{Connection, OptionalExtension, Result};
use std::collections::HashMap;

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

pub fn list(conn: &Connection) -> Result<Vec<PromptCard>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {PCARD_COLS}
         FROM prompts WHERE is_deleted = 0 ORDER BY updated_at DESC"
    ))?;
    let rows = stmt.query_map([], row_to_prompt_card)?;
    rows.collect()
}

/// 主页/回收站列表的卡片投影：与 `Prompt` 相比去掉恒定的 `is_deleted`
/// （未删除列表恒 false、回收站恒 true，前端零使用），与图像侧 `ImageCard` 对称。
/// `content` / `content_translate` / `note` 为卡片与详情编辑所需，保留在投影内。
#[derive(Debug, Serialize, Clone, specta::Type)]
pub struct PromptCard {
    pub id: String,
    pub title: String,
    pub content: String,
    pub content_translate: String,
    pub created_at: String,
    pub updated_at: String,
    pub deleted_at: Option<String>,
    pub is_favorite: bool,
    pub is_safe: bool,
    pub note: String,
}

/// 分页提示词列表：items 为本页，total 为符合条件总数。
#[derive(Debug, Serialize, Clone, specta::Type)]
pub struct PaginatedPrompts {
    pub items: Vec<PromptCard>,
    pub total: i64,
}

/// 卡片投影列：与 `PromptCard` 字段一一对应，顺序即 `row_to_prompt_card` 的取列顺序。
const PCARD_COLS: &str = "id, title, content, content_translate, created_at, updated_at, deleted_at, is_favorite, is_safe, note";

fn row_to_prompt_card(row: &rusqlite::Row) -> Result<PromptCard> {
    Ok(PromptCard {
        id: row.get(0)?,
        title: row.get(1)?,
        content: row.get(2)?,
        content_translate: row.get(3)?,
        created_at: row.get(4)?,
        updated_at: row.get(5)?,
        deleted_at: row.get(6)?,
        is_favorite: row.get(7)?,
        is_safe: row.get(8)?,
        note: row.get(9)?,
    })
}

/// 提示词关联的「未删除图像」数量（`img.is_deleted = 0`），特殊标签 无图 / 多图 按它判定。
const IMG_COUNT_SQL: &str = "SELECT COUNT(*) FROM prompt_image_relations pir JOIN images i ON i.id = pir.image_id WHERE pir.prompt_id = prompts.id AND i.is_deleted = 0";

/// 排序键白名单 → 列名（未命中回落 `updated_at`，与旧 `list` 的默认序一致）。
fn sort_column(sort: &str) -> &'static str {
    match sort {
        "createdAt" => "created_at",
        "title" => "title",
        _ => "updated_at",
    }
}

/// 分页查询的 WHERE 片段与绑定值：search（标题 / 内容 / 译文 / 备注 / 标签名）+ 所选标签（AND）+ 反选。
/// 搜索范围与前端 `matchesKeyword([title, content, content_translate, note], tagNames)` 对齐。
fn page_filter(q: &ListQuery) -> (String, Vec<Value>) {
    let mut sql = String::new();
    let mut params: Vec<Value> = Vec::new();

    let search = q.search.trim();
    if !search.is_empty() {
        // ESCAPE '\' 配合 escape_like：否则用户输入单个 % 会匹配全部、_ 匹配任意单字符
        sql.push_str(
            " AND (title LIKE ? ESCAPE '\\' OR content LIKE ? ESCAPE '\\' OR content_translate LIKE ? ESCAPE '\\' OR note LIKE ? ESCAPE '\\'
                   OR EXISTS (SELECT 1 FROM prompt_tag_relations r2 JOIN prompt_tags t2 ON t2.id = r2.tag_id WHERE r2.prompt_id = prompts.id AND t2.name LIKE ? ESCAPE '\\'))",
        );
        let like = format!("%{}%", crate::infra::text_utils::escape_like(search));
        for _ in 0..5 {
            params.push(Value::Text(like.clone()));
        }
    }

    let mut conds: Vec<String> = Vec::new();
    for raw in &q.tags {
        let name = raw.trim();
        if name.is_empty() {
            continue;
        }
        match name {
            list_query::SP_FAVORITE => conds.push("is_favorite = 1".to_string()),
            list_query::SP_MULTI_IMAGE => conds.push(format!("({IMG_COUNT_SQL}) > 1")),
            list_query::SP_NO_IMAGE => conds.push(format!("({IMG_COUNT_SQL}) = 0")),
            list_query::SP_NO_TAG => conds.push(
                "NOT EXISTS (SELECT 1 FROM prompt_tag_relations r WHERE r.prompt_id = prompts.id)"
                    .to_string(),
            ),
            list_query::SP_SINGLE_LANG => {
                conds.push("COALESCE(content_translate, '') = ''".to_string())
            }
            list_query::SP_SAFE => conds.push("is_safe = 1".to_string()),
            list_query::SP_UNSAFE => conds.push("is_safe = 0".to_string()),
            other => {
                conds.push(
                    "EXISTS (SELECT 1 FROM prompt_tag_relations r JOIN prompt_tags t ON t.id = r.tag_id WHERE r.prompt_id = prompts.id AND t.name = ?)"
                        .to_string(),
                );
                params.push(Value::Text(other.to_string()));
            }
        }
    }
    if !conds.is_empty() {
        let joined = conds.join(" AND ");
        if q.inverted {
            sql.push_str(&format!(" AND NOT ({joined})"));
        } else {
            sql.push_str(&format!(" AND {joined}"));
        }
    }
    (sql, params)
}

/// 主页分页列表：排序 / 搜索 / 标签筛选（含特殊标签）全部下推到 SQL，返回本页与总数。
pub fn list_page(conn: &Connection, q: &ListQuery) -> Result<PaginatedPrompts> {
    let (filter, mut params) = page_filter(q);
    let total: i64 = conn.query_row(
        &format!("SELECT COUNT(*) FROM prompts WHERE is_deleted = 0{filter}"),
        rusqlite::params_from_iter(params.iter()),
        |r| r.get(0),
    )?;

    let sql = format!(
        "SELECT {PCARD_COLS} FROM prompts WHERE is_deleted = 0{filter}
         ORDER BY {} {} LIMIT ? OFFSET ?",
        sort_column(&q.sort),
        list_query::order_dir(q.desc)
    );
    params.push(Value::Integer(list_query::clamp_limit(q.limit)));
    params.push(Value::Integer(list_query::clamp_offset(q.offset)));
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(
        rusqlite::params_from_iter(params.iter()),
        row_to_prompt_card,
    )?;
    Ok(PaginatedPrompts {
        items: rows.collect::<Result<Vec<_>>>()?,
        total,
    })
}

/// 同条件只取 id（全选 / 反选 / 批量操作用），条数封顶 `MAX_IDS`。
pub fn list_ids(conn: &Connection, q: &ListQuery) -> Result<Vec<String>> {
    let (filter, params) = page_filter(q);
    let sql = format!(
        "SELECT id FROM prompts WHERE is_deleted = 0{filter}
         ORDER BY {} {} LIMIT {} OFFSET 0",
        sort_column(&q.sort),
        list_query::order_dir(q.desc),
        list_query::MAX_IDS
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(rusqlite::params_from_iter(params.iter()), |r| r.get(0))?;
    rows.collect()
}

/// 特殊标签命中数（基于全部未删除提示词，不含搜索 / 标签条件，与前端 specialCounts 口径一致）。
pub fn special_counts(conn: &Connection) -> Result<HashMap<String, i64>> {
    let sql = format!(
        "SELECT
           COUNT(CASE WHEN is_favorite = 1 THEN 1 END),
           COUNT(CASE WHEN ({IMG_COUNT_SQL}) > 1 THEN 1 END),
           COUNT(CASE WHEN ({IMG_COUNT_SQL}) = 0 THEN 1 END),
           COUNT(CASE WHEN is_safe = 1 THEN 1 END),
           COUNT(CASE WHEN is_safe = 0 THEN 1 END),
           COUNT(CASE WHEN COALESCE(content_translate, '') = '' THEN 1 END),
           COUNT(CASE WHEN NOT EXISTS (SELECT 1 FROM prompt_tag_relations r WHERE r.prompt_id = prompts.id) THEN 1 END)
         FROM prompts WHERE is_deleted = 0"
    );
    let (fav, multi, no_img, safe, unsafe_, single, no_tag) = conn.query_row(&sql, [], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, i64>(1)?,
            r.get::<_, i64>(2)?,
            r.get::<_, i64>(3)?,
            r.get::<_, i64>(4)?,
            r.get::<_, i64>(5)?,
            r.get::<_, i64>(6)?,
        ))
    })?;
    let mut m = HashMap::new();
    m.insert(list_query::SP_FAVORITE.to_string(), fav);
    m.insert(list_query::SP_MULTI_IMAGE.to_string(), multi);
    m.insert(list_query::SP_NO_IMAGE.to_string(), no_img);
    m.insert(list_query::SP_SAFE.to_string(), safe);
    m.insert(list_query::SP_UNSAFE.to_string(), unsafe_);
    m.insert(list_query::SP_SINGLE_LANG.to_string(), single);
    m.insert(list_query::SP_NO_TAG.to_string(), no_tag);
    Ok(m)
}

/// 软删除：标记为已删除（与图像回收站机制一致）。
/// 软删除是明显的更新操作，同步刷新 updated_at；关联图像的「关联提示词」列表会因
/// is_deleted 过滤少一条（隐式解绑，与 purge 语义一致），同步刷对方 updated_at。
pub fn remove(conn: &Connection, id: &str) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "UPDATE prompts
         SET is_deleted = 1,
             deleted_at = strftime('%Y-%m-%dT%H:%M:%fZ','now'),
             updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now')
         WHERE id = ?1",
        rusqlite::params![id],
    )?;
    tx.execute(
        "UPDATE images SET updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now')
         WHERE id IN (SELECT image_id FROM prompt_image_relations WHERE prompt_id = ?1)",
        rusqlite::params![id],
    )?;
    tx.commit()?;
    Ok(())
}

/// 列出回收站中的提示词（已软删除），按删除时间倒序。
pub fn list_trashed(conn: &Connection) -> Result<Vec<PromptCard>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {PCARD_COLS}
         FROM prompts WHERE is_deleted = 1 ORDER BY deleted_at DESC"
    ))?;
    let rows = stmt.query_map([], row_to_prompt_card)?;
    rows.collect()
}

/// 恢复软删除的提示词。恢复是明显的更新操作，同步刷新 updated_at；
/// 关联图像视为隐式重新关联，一并刷新对方 updated_at。
pub fn restore(conn: &Connection, id: &str) -> Result<Option<Prompt>> {
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "UPDATE prompts
         SET is_deleted = 0, deleted_at = NULL,
             updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now')
         WHERE id = ?1",
        rusqlite::params![id],
    )?;
    tx.execute(
        "UPDATE images SET updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now')
         WHERE id IN (SELECT image_id FROM prompt_image_relations WHERE prompt_id = ?1)",
        rusqlite::params![id],
    )?;
    tx.commit()?;
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

/// 恢复全部回收站提示词，返回恢复数量。恢复是明显的更新操作，同步刷新 updated_at；
/// 关联图像视为隐式重新关联，一并刷新对方 updated_at。
/// 先刷后恢复：恢复完成后无法区分哪些提示词刚被恢复，会误刷无关图像。
pub fn restore_all(conn: &Connection) -> Result<usize> {
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "UPDATE images SET updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now')
         WHERE id IN (SELECT pir.image_id FROM prompt_image_relations pir
                      JOIN prompts p ON p.id = pir.prompt_id WHERE p.is_deleted = 1)",
        [],
    )?;
    let count = tx.execute(
        "UPDATE prompts
         SET is_deleted = 0, deleted_at = NULL,
             updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now')
         WHERE is_deleted = 1",
        [],
    )?;
    tx.commit()?;
    Ok(count)
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

/// 提示词卡片背景缩略图：{promptId: 相对路径}，取每个提示词第一张**有缩略图**的关联（未删除）图像。
/// 返回 `images.thumbnail_path` 原值（相对数据目录），由调用方拼绝对路径（命令层给前端、
/// 自愈内部比对都基于它）。thumbnail_path 为 NULL 的记录（如导入时无法解码的文件）自动跳过，
/// 让位给后续可用图像，且不会因 NULL 阻塞整个映射。
/// `ids` 为当前已加载块内的提示词（主页按块取，不做全量映射）。
pub fn thumbs_for(conn: &Connection, ids: &[String]) -> Result<HashMap<String, String>> {
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    let placeholders = vec!["?"; ids.len()].join(",");
    let sql = format!(
        "SELECT prompt_id, thumbnail_path
         FROM (
            SELECT pir.prompt_id AS prompt_id, img.thumbnail_path AS thumbnail_path,
                   ROW_NUMBER() OVER (PARTITION BY pir.prompt_id ORDER BY pir.sort_order, pir.rowid) AS rn
            FROM prompt_image_relations pir
            JOIN images img ON img.id = pir.image_id
            WHERE img.is_deleted = 0 AND img.thumbnail_path IS NOT NULL
              AND pir.prompt_id IN ({placeholders})
         )
         WHERE rn = 1"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(rusqlite::params_from_iter(ids.iter()), |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
    })?;
    let mut map: HashMap<String, String> = HashMap::new();
    for row in rows {
        let (pid, thumb_rel) = row?;
        map.insert(pid, thumb_rel);
    }
    Ok(map)
}

/// 提示词卡片背景懒自愈：按提示词取其关联（未删除）图像，缺缩略图的按需生成并回写，
/// 返回**背景路径发生变化**的提示词（本次新补齐，或首图来源换了一张），
/// `thumbnail_path` 为相对数据目录的路径（与图像侧 `thumbnail_service::ensure` 同义）。
/// 调用方为浏览页的可见窗口，顺序执行即可。
pub fn ensure_thumbnails(
    conn: &Connection,
    data_dir: &std::path::Path,
    thumbs_root: &std::path::Path,
    ids: &[String],
) -> std::result::Result<Vec<ThumbnailEnsureFixed>, String> {
    let before = thumbs_for(conn, ids).map_err(|e| e.to_string())?;
    let image_ids = related_image_ids(conn, ids).map_err(|e| e.to_string())?;
    if !image_ids.is_empty() {
        thumbnail_service::ensure(data_dir, thumbs_root, conn, &image_ids)?;
    }
    let after = thumbs_for(conn, ids).map_err(|e| e.to_string())?;
    Ok(after
        .into_iter()
        .filter(|(pid, path)| before.get(pid) != Some(path))
        .map(|(id, thumbnail_path)| ThumbnailEnsureFixed { id, thumbnail_path })
        .collect())
}

/// 这些提示词关联的全部（未删除）图像 id（去重），供缩略图自愈。
fn related_image_ids(conn: &Connection, prompt_ids: &[String]) -> Result<Vec<String>> {
    if prompt_ids.is_empty() {
        return Ok(Vec::new());
    }
    let placeholders = vec!["?"; prompt_ids.len()].join(",");
    let sql = format!(
        "SELECT DISTINCT img.id
         FROM prompt_image_relations pir
         JOIN images img ON img.id = pir.image_id
         WHERE pir.prompt_id IN ({placeholders}) AND img.is_deleted = 0"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(rusqlite::params_from_iter(prompt_ids.iter()), |r| r.get(0))?;
    rows.collect()
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
