//! 图像领域服务：承载图像存储（落盘 + 缩略图 + 入库）与查询的业务规则。
//! 领域层感知 app（用于定位数据目录）与连接访问数据，但不直接面向 IPC。
//! 命令层见 `commands::image`。

use crate::domain::image_ops::{make_center_thumb, open_image};
use crate::domain::list_query::{self, ListQuery};
use crate::domain::tag_manager::{tags_by_owner, TagDomain};
use crate::infra::error::AppError;
use image::GenericImageView;
use rusqlite::types::Value;
use rusqlite::{Connection, OptionalExtension, Result};
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize, Clone, specta::Type)]
pub struct Image {
    pub id: String,
    pub file_name: String,
    pub stored_name: String,
    pub relative_path: String,
    pub thumbnail_path: Option<String>,
    pub md5: Option<String>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    /// 文件字节数（specta 的 BigInt 类型经 Builder 配置导出为 TS number）。
    pub file_size: i64,
    pub gen_params: String,
    pub is_deleted: bool,
    pub deleted_at: Option<String>,
    pub is_favorite: bool,
    pub is_safe: bool,
    pub created_at: String,
    pub updated_at: String,
    pub note: String,
}

#[derive(Debug, Serialize, Clone, specta::Type)]
pub struct ImageImportResult {
    pub image: Image,
    pub is_duplicate: bool,
}

#[derive(Debug, Serialize, Clone, specta::Type)]
pub struct ImageImportError {
    pub path: String,
    pub message: String,
}

#[derive(Debug, Serialize, Clone, specta::Type)]
pub struct ImageImportBatchResult {
    pub results: Vec<ImageImportResult>,
    pub errors: Vec<ImageImportError>,
}

/// 主页/回收站/选图列表的卡片投影：详情才用的重字段（stored_name/relative_path/
/// md5/gen_params）不出库，降低万级列表的内存与序列化开销（todo P3）。
/// `stored_name` 仅后台拼路径与排序键使用，前端显示一律用 file_name。
#[derive(Debug, Serialize, Clone, specta::Type)]
pub struct ImageCard {
    pub id: String,
    pub file_name: String,
    pub thumbnail_path: Option<String>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    /// 文件字节数（specta 的 BigInt 类型经 Builder 配置导出为 TS number）。
    pub file_size: i64,
    pub is_favorite: bool,
    pub is_safe: bool,
    pub note: String,
    pub created_at: String,
    pub updated_at: String,
    pub deleted_at: Option<String>,
}

/// 分页图像列表：items 为本页图像，total 为总数（供「从图像列表导入」信息栏使用）。
#[derive(Debug, Serialize, Clone, specta::Type)]
pub struct PaginatedImages {
    pub items: Vec<ImageCard>,
    pub total: i64,
}

#[derive(Debug, Serialize, Clone, specta::Type)]
pub struct LinkedPrompt {
    pub id: String,
    pub title: String,
    pub content: String,
    pub content_translate: String,
    pub note: String,
    pub is_favorite: bool,
    pub is_safe: bool,
    pub tags: Vec<String>,
}

/// 支持导入的图像格式扩展名（小写）。唯一定义处：
/// `select_images` 对话框过滤、导入校验（ext_ok）与解码失败报错文案均由此派生；
/// README 的「支持的图像格式」矩阵为文档，需手动同步。
pub const SUPPORTED_EXT: &[&str] = &[
    "png", "jpg", "jpeg", "gif", "webp", "bmp", "ico", "tif", "tiff",
];

fn ext_ok(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| SUPPORTED_EXT.contains(&e.to_lowercase().as_str()))
        .unwrap_or(false)
}

/// 复制源图到 images/<年月>/（stored_name 与 pm 一致，为 "{id}{ext}"），
/// 生成 200×200 居中裁剪的 jpeg 缩略图到 thumbnails/<年月>/thumb_{id}.jpg，入库，返回记录。
/// 返回 `(记录, 是否重复)`：重复时复用已存在记录且不落新文件。
pub fn import(
    conn: &Connection,
    app: &tauri::AppHandle,
    source: &str,
) -> rusqlite::Result<(Image, bool)> {
    import_with(
        conn,
        &crate::infra::db::images_dir(app),
        &crate::infra::db::thumbnails_dir(app),
        source,
    )
}

/// import 的路径注入版（供测试），app 依赖仅用于定位数据目录。
pub(crate) fn import_with(
    conn: &Connection,
    images_dir: &Path,
    thumbnails_dir: &Path,
    source: &str,
) -> rusqlite::Result<(Image, bool)> {
    let source = PathBuf::from(source);
    if !source.is_file() {
        return Err(rusqlite::Error::InvalidParameterName(
            "源文件不存在".to_string(),
        ));
    }
    if !ext_ok(&source) {
        return Err(rusqlite::Error::InvalidParameterName(
            "不支持的图片格式".to_string(),
        ));
    }

    // MD5 去重：与已入库图像内容相同则复用（含回收站记录，自动恢复）
    let md5 = file_md5(&source)?;
    if let Some(existing) = find_by_md5(conn, &md5)? {
        let img = if existing.is_deleted {
            let img = restore(conn, &existing.id)?.ok_or(rusqlite::Error::QueryReturnedNoRows)?;
            img
        } else {
            existing
        };
        return Ok((img, true));
    }

    // 先解码验证：拒绝扩展名伪装或损坏的文件。解码失败的文件若静默入库，
    // thumbnail_path 为 NULL，会让提示词页的卡片背景整批失效。
    let img = open_image(&source).map_err(|e| {
        rusqlite::Error::InvalidParameterName(format!(
            "无法解析图像文件（可能已损坏或为不支持的格式；当前支持 {}，AVIF/HEIC/SVG 请先转换）: {e}",
            SUPPORTED_EXT.join(" / ")
        ))
    })?;

    std::fs::create_dir_all(images_dir).map_err(io_to_sql)?;
    std::fs::create_dir_all(thumbnails_dir).map_err(io_to_sql)?;

    let md = std::fs::metadata(&source).map_err(io_to_sql)?;
    let file_size = md.len() as i64;

    // 6 位年月子目录：images/YYYYMM/
    let yyyymm = unix_to_yyyymm(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64,
    );
    let month_dir = images_dir.join(&yyyymm);
    std::fs::create_dir_all(&month_dir).map_err(io_to_sql)?;

    let ext = source
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("png")
        .to_lowercase();
    // stored_name 与 pm 一致："{imageId}{ext}"，缩略图随之命名
    let id = crate::infra::db::gen_id(crate::infra::db::IMAGE_ID_PREFIX);
    let stored_name = format!("{id}.{ext}");
    let dest = month_dir.join(&stored_name);
    std::fs::copy(&source, &dest).map_err(io_to_sql)?;

    // 生成居中裁剪的方图缩略图（生成失败仅缺缩略图，不阻断导入）
    let (width, height, thumb_rel) = {
        let (w, h) = img.dimensions();
        match make_center_thumb(&img) {
            Ok(thumb) => {
                let thumb_name = format!("thumb_{id}.jpg");
                let thumb_month_dir = thumbnails_dir.join(&yyyymm);
                std::fs::create_dir_all(&thumb_month_dir).map_err(io_to_sql)?;
                let thumb_abs = thumb_month_dir.join(&thumb_name);
                thumb.save(&thumb_abs).map_err(|e| {
                    rusqlite::Error::ToSqlConversionFailure(Box::new(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        format!("生成缩略图失败: {e}"),
                    )))
                })?;
                (
                    Some(w as i64),
                    Some(h as i64),
                    Some(format!("thumbnails/{yyyymm}/{thumb_name}")),
                )
            }
            Err(_) => (Some(w as i64), Some(h as i64), None),
        }
    };

    let relative_path = format!("images/{yyyymm}/{stored_name}");
    let file_name = source
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(&stored_name)
        .to_string();
    conn.execute(
        "INSERT INTO images(id, file_name, stored_name, relative_path, thumbnail_path, md5, width, height, file_size)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        rusqlite::params![
            id,
            file_name,
            stored_name,
            relative_path,
            thumb_rel,
            md5,
            width,
            height,
            file_size
        ],
    )?;
    let new_img = get_by_id(conn, &id)?.ok_or(rusqlite::Error::QueryReturnedNoRows)?;
    Ok((new_img, false))
}

/// 拼接搜索/标签的 WHERE 条件子句与参数（search 匹配文件名/备注/标签名，tag 需存在同名标签关联）。
fn filter_sql(search: Option<&str>, tag: Option<&str>) -> (String, Vec<String>) {
    let mut clauses = String::new();
    let mut params = Vec::new();
    let search = search.unwrap_or("").trim();
    if !search.is_empty() {
        // ESCAPE '\' 配合 escape_like：否则用户输入单个 % 会匹配全部、_ 匹配任意单字符
        clauses.push_str(
            " AND (file_name LIKE ? ESCAPE '\\' OR note LIKE ? ESCAPE '\\' OR EXISTS (SELECT 1 FROM image_tag_relations r2 JOIN image_tags t2 ON t2.id = r2.tag_id WHERE r2.image_id = images.id AND t2.name LIKE ? ESCAPE '\\'))",
        );
        let like = format!("%{}%", crate::infra::text_utils::escape_like(search));
        params.push(like.clone());
        params.push(like.clone());
        params.push(like);
    }
    let tag = tag.unwrap_or("").trim();
    if !tag.is_empty() {
        clauses.push_str(
            " AND EXISTS (SELECT 1 FROM image_tag_relations r JOIN image_tags t ON t.id = r.tag_id WHERE r.image_id = images.id AND t.name = ?)",
        );
        params.push(tag.to_string());
    }
    (clauses, params)
}

/// 非软删除图像列表；search/tag 参与 SQL 过滤，limit 为 Some(n) 时只取前 n 张（按创建时间倒序）。
pub fn list(
    conn: &Connection,
    search: Option<&str>,
    tag: Option<&str>,
    limit: Option<i64>,
) -> rusqlite::Result<Vec<ImageCard>> {
    let (clauses, mut params) = filter_sql(search, tag);
    let mut sql = format!(
        "SELECT {CARD_COLS}
         FROM images WHERE is_deleted = 0{clauses} ORDER BY created_at DESC"
    );
    if let Some(n) = limit {
        sql.push_str(" LIMIT ?");
        params.push(n.to_string());
    }
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(rusqlite::params_from_iter(params), row_to_card)?;
    rows.collect()
}

/// 非软删除图像总数（与 list 相同的搜索/标签条件；分页信息栏用）。
pub fn count(conn: &Connection, search: Option<&str>, tag: Option<&str>) -> rusqlite::Result<i64> {
    let (clauses, params) = filter_sql(search, tag);
    let sql = format!("SELECT COUNT(*) FROM images WHERE is_deleted = 0{clauses}");
    conn.query_row(&sql, rusqlite::params_from_iter(params), |r| r.get(0))
}

const IMAGE_COLS: &str = "id, file_name, stored_name, relative_path, thumbnail_path, md5, width, height, file_size, gen_params, is_deleted, deleted_at, is_favorite, is_safe, created_at, updated_at, note";

/// 卡片投影列：与 `ImageCard` 字段一一对应，顺序即 `row_to_card` 的取列顺序。
const CARD_COLS: &str = "id, file_name, thumbnail_path, width, height, file_size, is_favorite, is_safe, note, created_at, updated_at, deleted_at";

fn row_to_card(r: &rusqlite::Row) -> rusqlite::Result<ImageCard> {
    Ok(ImageCard {
        id: r.get(0)?,
        file_name: r.get(1)?,
        thumbnail_path: r.get(2)?,
        width: r.get(3)?,
        height: r.get(4)?,
        file_size: r.get(5)?,
        is_favorite: r.get(6)?,
        is_safe: r.get(7)?,
        note: r.get(8)?,
        created_at: r.get(9)?,
        updated_at: r.get(10)?,
        deleted_at: r.get(11)?,
    })
}

/// 排序键白名单 → 列名（未命中回落 `created_at`）。
/// 时间列按 ISO 8601 UTC 字符串排序：paim 原生与 pm 导入（经 `normalize_ts`）均为该格式，字典序即时间序。
fn sort_column(sort: &str) -> &'static str {
    match sort {
        "updatedAt" => "updated_at",
        "fileSize" => "file_size",
        "fileName" => "stored_name",
        "width" => "COALESCE(width, 0)",
        "height" => "COALESCE(height, 0)",
        _ => "created_at",
    }
}

/// 分页查询的 WHERE 片段与绑定值：search（file_name / note / 标签名）+ 所选标签（AND）+ 反选。
/// 特殊标签落成 SQL 条件，普通标签落成 EXISTS 子查询；反选时整体取 NOT。
fn page_filter(q: &ListQuery) -> (String, Vec<Value>) {
    let mut sql = String::new();
    let mut params: Vec<Value> = Vec::new();

    // search 复用 filter_sql（已带 ESCAPE '\'）；标签部分由下方 tags 统一处理，故传 None
    let (search_sql, search_params) = filter_sql(Some(&q.search), None);
    sql.push_str(&search_sql);
    params.extend(list_query::to_values(search_params));

    let mut conds: Vec<String> = Vec::new();
    for raw in &q.tags {
        let name = raw.trim();
        if name.is_empty() {
            continue;
        }
        match name {
            list_query::SP_FAVORITE => conds.push("is_favorite = 1".to_string()),
            list_query::SP_UNREFERENCED => conds.push(
                // 不相关子查询：SQLite 只物化一次再逐行查，逐行相关子查询会退化到分钟级
                "images.id NOT IN (SELECT pir.image_id FROM prompt_image_relations pir \
                 JOIN prompts p ON p.id = pir.prompt_id AND p.is_deleted = 0)"
                    .to_string(),
            ),
            list_query::SP_MULTI_REF => conds.push(
                "images.id IN (SELECT pir.image_id FROM prompt_image_relations pir \
                 JOIN prompts p ON p.id = pir.prompt_id AND p.is_deleted = 0 \
                 GROUP BY pir.image_id HAVING COUNT(*) > 1)"
                    .to_string(),
            ),
            list_query::SP_NO_TAG => conds.push(
                "NOT EXISTS (SELECT 1 FROM image_tag_relations r WHERE r.image_id = images.id)"
                    .to_string(),
            ),
            list_query::SP_SAFE => conds.push("is_safe = 1".to_string()),
            list_query::SP_UNSAFE => conds.push("is_safe = 0".to_string()),
            other => {
                conds.push(
                    "EXISTS (SELECT 1 FROM image_tag_relations r JOIN image_tags t ON t.id = r.tag_id WHERE r.image_id = images.id AND t.name = ?)"
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
/// 与前端「块拉取」配套：一次只取 `limit` 条，`total` 用来撑起滚动条与占位数量。
pub fn list_page(conn: &Connection, q: &ListQuery) -> Result<PaginatedImages> {
    let (filter, mut params) = page_filter(q);
    let total: i64 = conn.query_row(
        &format!("SELECT COUNT(*) FROM images WHERE is_deleted = 0{filter}"),
        rusqlite::params_from_iter(params.iter()),
        |r| r.get(0),
    )?;

    let sql = format!(
        "SELECT {CARD_COLS} FROM images WHERE is_deleted = 0{filter}
         ORDER BY {} {} LIMIT ? OFFSET ?",
        sort_column(&q.sort),
        list_query::order_dir(q.desc)
    );
    params.push(Value::Integer(list_query::clamp_limit(q.limit)));
    params.push(Value::Integer(list_query::clamp_offset(q.offset)));
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(rusqlite::params_from_iter(params.iter()), row_to_card)?;
    Ok(PaginatedImages {
        items: rows.collect::<Result<Vec<_>>>()?,
        total,
    })
}

/// 同条件只取 id（全选 / 反选 / 批量操作用），条数封顶 `MAX_IDS`。
pub fn list_ids(conn: &Connection, q: &ListQuery) -> Result<Vec<String>> {
    let (filter, params) = page_filter(q);
    let sql = format!(
        "SELECT id FROM images WHERE is_deleted = 0{filter}
         ORDER BY {} {} LIMIT {} OFFSET 0",
        sort_column(&q.sort),
        list_query::order_dir(q.desc),
        list_query::MAX_IDS
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(rusqlite::params_from_iter(params.iter()), |r| r.get(0))?;
    rows.collect()
}

/// 特殊标签命中数（基于全部未删除图像，不含搜索 / 标签条件，与前端 specialCounts 口径一致）。
/// 引用计数不走逐行相关子查询（含 JOIN prompts 的相关子查询万级数据下退化到分钟级，
/// 详见 statistics_service::image_special_counts 同款修复），改派生表聚合：10000 图 90ms。
pub fn special_counts(conn: &Connection) -> Result<HashMap<String, i64>> {
    let sql = "SELECT
       COUNT(CASE WHEN i.is_favorite = 1 THEN 1 END),
       COUNT(CASE WHEN COALESCE(pr.cnt, 0) = 0 THEN 1 END),
       COUNT(CASE WHEN COALESCE(pr.cnt, 0) > 1 THEN 1 END),
       COUNT(CASE WHEN i.is_safe = 1 THEN 1 END),
       COUNT(CASE WHEN i.is_safe = 0 THEN 1 END),
       COUNT(CASE WHEN COALESCE(tr.cnt, 0) = 0 THEN 1 END)
     FROM images i
     LEFT JOIN (SELECT pir.image_id, COUNT(*) cnt
                FROM prompt_image_relations pir
                JOIN prompts p ON p.id = pir.prompt_id AND p.is_deleted = 0
                GROUP BY pir.image_id) pr ON pr.image_id = i.id
     LEFT JOIN (SELECT itr.image_id, COUNT(*) cnt
                FROM image_tag_relations itr
                GROUP BY itr.image_id) tr ON tr.image_id = i.id
     WHERE i.is_deleted = 0";
    let (fav, unref, multi, safe, unsafe_, no_tag) = conn.query_row(sql, [], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, i64>(1)?,
            r.get::<_, i64>(2)?,
            r.get::<_, i64>(3)?,
            r.get::<_, i64>(4)?,
            r.get::<_, i64>(5)?,
        ))
    })?;
    let mut m = HashMap::new();
    m.insert(list_query::SP_FAVORITE.to_string(), fav);
    m.insert(list_query::SP_UNREFERENCED.to_string(), unref);
    m.insert(list_query::SP_MULTI_REF.to_string(), multi);
    m.insert(list_query::SP_SAFE.to_string(), safe);
    m.insert(list_query::SP_UNSAFE.to_string(), unsafe_);
    m.insert(list_query::SP_NO_TAG.to_string(), no_tag);
    Ok(m)
}

/// 回收站列表（软删除的图像）。
pub fn list_trashed(conn: &Connection) -> rusqlite::Result<Vec<ImageCard>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {CARD_COLS}
         FROM images WHERE is_deleted = 1 ORDER BY deleted_at DESC"
    ))?;
    let rows = stmt.query_map([], row_to_card)?;
    rows.collect()
}

/// 软删除：标记为已删除，保留文件以便恢复。
/// 软删除是明显的更新操作，同步刷新 updated_at；关联提示词的「关联图像」列表会因
/// is_deleted 过滤少一条（隐式解绑，与 purge 语义一致），同步刷对方 updated_at。
pub fn soft_delete(conn: &Connection, id: &str) -> rusqlite::Result<Option<Image>> {
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "UPDATE images
         SET is_deleted = 1,
             deleted_at = strftime('%Y-%m-%dT%H:%M:%fZ','now'),
             updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now')
         WHERE id = ?1",
        rusqlite::params![id],
    )?;
    tx.execute(
        "UPDATE prompts SET updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now')
         WHERE id IN (SELECT prompt_id FROM prompt_image_relations WHERE image_id = ?1)",
        rusqlite::params![id],
    )?;
    tx.commit()?;
    get_by_id(conn, id)
}

/// 恢复软删除的图像。恢复是明显的更新操作，同步刷新 updated_at；
/// 关联提示词视为隐式重新关联，一并刷新对方 updated_at。
pub fn restore(conn: &Connection, id: &str) -> rusqlite::Result<Option<Image>> {
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "UPDATE images
         SET is_deleted = 0, deleted_at = NULL,
             updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now')
         WHERE id = ?1",
        rusqlite::params![id],
    )?;
    tx.execute(
        "UPDATE prompts SET updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now')
         WHERE id IN (SELECT prompt_id FROM prompt_image_relations WHERE image_id = ?1)",
        rusqlite::params![id],
    )?;
    tx.commit()?;
    get_by_id(conn, id)
}

/// 批量操作结果：成功数与失败数。
#[derive(Debug, Serialize, Clone, specta::Type)]
pub struct TrashBatchResult {
    pub count: usize,
    pub failures: usize,
}

/// 恢复全部回收站图像，返回恢复数量。恢复是明显的更新操作，同步刷新 updated_at；
/// 关联提示词视为隐式重新关联，一并刷新对方 updated_at。
/// 先刷后恢复：恢复完成后无法区分哪些图像刚被恢复，会误刷无关提示词。
pub fn restore_all(conn: &Connection) -> rusqlite::Result<usize> {
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "UPDATE prompts SET updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now')
         WHERE id IN (SELECT pir.prompt_id FROM prompt_image_relations pir
                      JOIN images i ON i.id = pir.image_id WHERE i.is_deleted = 1)",
        [],
    )?;
    let count = tx.execute(
        "UPDATE images
         SET is_deleted = 0, deleted_at = NULL,
             updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now')
         WHERE is_deleted = 1",
        [],
    )?;
    tx.commit()?;
    Ok(count)
}

/// 清空回收站：逐项彻底删除（含磁盘原图与缩略图），逐项容错。
pub fn empty_trash(conn: &Connection, app: &tauri::AppHandle) -> TrashBatchResult {
    let ids: Vec<String> = match list_trashed(conn) {
        Ok(items) => items.into_iter().map(|i| i.id).collect(),
        Err(_) => {
            return TrashBatchResult {
                count: 0,
                failures: 0,
            }
        }
    };
    let mut result = TrashBatchResult {
        count: 0,
        failures: 0,
    };
    for id in ids {
        match purge(conn, app, &id) {
            Ok(_) => result.count += 1,
            Err(_) => result.failures += 1,
        }
    }
    result
}

/// 更新图像详情字段（文件名、备注、收藏、安全评级）。仅更新传入 `Some` 的字段，未传的字段不动。
/// 非空校验（文件名必填）与逐字段脏检查由前端负责，前端只把真正变化的字段传进来；后端不再校验、不再比较。
/// `Some("")` 表示显式清空（备注允许），与「不更新」的 `None` 语义区分。
pub fn update_detail(
    conn: &Connection,
    id: &str,
    file_name: Option<&str>,
    note: Option<&str>,
    is_favorite: Option<bool>,
    is_safe: Option<bool>,
) -> std::result::Result<Option<Image>, AppError> {
    let mut sets: Vec<String> = Vec::new();
    let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
    if let Some(v) = file_name {
        sets.push("file_name = ?".into());
        params.push(Box::new(v.trim().to_string()));
    }
    if let Some(v) = note {
        sets.push("note = ?".into());
        params.push(Box::new(v.to_string()));
    }
    if let Some(v) = is_favorite {
        sets.push("is_favorite = ?".into());
        params.push(Box::new(if v { 1 } else { 0 }));
    }
    if let Some(v) = is_safe {
        sets.push("is_safe = ?".into());
        params.push(Box::new(if v { 1 } else { 0 }));
    }

    // 没有任何字段需要更新：不写库，updated_at 保持不变
    if sets.is_empty() {
        return Ok(get_by_id(conn, id)?);
    }

    sets.push("updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now')".into());
    let sql = format!("UPDATE images SET {} WHERE id = ?", sets.join(", "));
    params.push(Box::new(id.to_string()));

    let mut stmt = conn.prepare(&sql)?;
    let param_refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|b| b.as_ref()).collect();
    stmt.execute(param_refs.as_slice())?;
    Ok(get_by_id(conn, id)?)
}

/// 彻底删除：删除磁盘原图与缩略图并移除记录，不可恢复。
/// 级联删除的 prompt_image_relations 视为隐式解绑，同步刷新关联提示词的 updated_at。
pub fn purge(conn: &Connection, app: &tauri::AppHandle, id: &str) -> rusqlite::Result<()> {
    purge_with(conn, &crate::infra::db::data_dir(app), id)
}

/// purge 的路径注入版（供测试），app 依赖仅用于定位数据目录。
pub(crate) fn purge_with(conn: &Connection, data_dir: &Path, id: &str) -> rusqlite::Result<()> {
    let row: Option<(Option<String>, Option<String>)> = conn
        .query_row(
            "SELECT relative_path, thumbnail_path FROM images WHERE id = ?1",
            rusqlite::params![id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;

    if let Some((rel, thumb)) = row {
        if let Some(rel) = rel {
            let _ = std::fs::remove_file(data_dir.join(rel));
        }
        if let Some(thumb) = thumb {
            let _ = std::fs::remove_file(data_dir.join(thumb));
        }
        let tx = conn.unchecked_transaction()?;
        let related_prompt_ids: Vec<String> = {
            let mut stmt =
                tx.prepare("SELECT prompt_id FROM prompt_image_relations WHERE image_id = ?1")?;
            let rows = stmt.query_map(rusqlite::params![id], |r| r.get(0))?;
            rows.collect::<rusqlite::Result<Vec<String>>>()?
        };
        tx.execute("DELETE FROM images WHERE id = ?1", rusqlite::params![id])?;
        if !related_prompt_ids.is_empty() {
            let placeholders = vec!["?"; related_prompt_ids.len()].join(",");
            tx.execute(
                &format!(
                    "UPDATE prompts SET updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now')
                     WHERE id IN ({placeholders})"
                ),
                rusqlite::params_from_iter(related_prompt_ids.iter()),
            )?;
        }
        tx.commit()?;
    }
    Ok(())
}

pub fn get_by_id(conn: &Connection, id: &str) -> rusqlite::Result<Option<Image>> {
    let mut stmt = conn.prepare(
        "SELECT id, file_name, stored_name, relative_path, thumbnail_path, md5, width, height, file_size, gen_params, is_deleted, deleted_at, is_favorite, is_safe, created_at, updated_at, note
         FROM images WHERE id = ?1",
    )?;
    let mut rows = stmt.query_map(rusqlite::params![id], row_to_image)?;
    rows.next().transpose()
}

fn find_by_md5(conn: &Connection, md5: &str) -> rusqlite::Result<Option<Image>> {
    let mut stmt = conn.prepare(
        "SELECT id, file_name, stored_name, relative_path, thumbnail_path, md5, width, height, file_size, gen_params, is_deleted, deleted_at, is_favorite, is_safe, created_at, updated_at, note
         FROM images WHERE md5 = ?1",
    )?;
    let mut rows = stmt.query_map(rusqlite::params![md5], row_to_image)?;
    rows.next().transpose()
}

/// 替换结果：SameImage 表示新图与旧图为同一张（MD5 相同），无需替换。
#[derive(Debug, Serialize, Clone, specta::Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ImageReplaceOutcome {
    SameImage,
    Replaced {
        image: Image,
        related_prompt_ids: Vec<String>,
    },
}

/// 替换图像（对齐 pm）：
/// 1. 新图走标准入库管线（复用 import：格式校验、MD5 去重、缩略图）
/// 2. 新图与旧图为同一张（MD5 命中复用旧 id）→ SameImage
/// 3. 否则事务内软删旧图、迁移提示词/标签关联、迁移 note/is_favorite/is_safe、
///    更新新图与关联提示词的 updated_at；旧图进回收站可恢复，物理文件保留。
pub fn replace_image(
    conn: &Connection,
    app: &tauri::AppHandle,
    old_id: &str,
    source: &str,
) -> std::result::Result<ImageReplaceOutcome, AppError> {
    replace_image_with(
        conn,
        &crate::infra::db::images_dir(app),
        &crate::infra::db::thumbnails_dir(app),
        old_id,
        source,
    )
}

/// replace_image 的路径注入版（供测试），app 依赖仅用于定位数据目录。
pub(crate) fn replace_image_with(
    conn: &Connection,
    images_dir: &Path,
    thumbnails_dir: &Path,
    old_id: &str,
    source: &str,
) -> std::result::Result<ImageReplaceOutcome, AppError> {
    // 旧图必须存在，防止对无效 id 操作
    if get_by_id(conn, old_id)?.is_none() {
        return Err(AppError::Message(format!("图像 {old_id} 不存在")));
    }

    let (new_img, is_duplicate) =
        import_with(conn, images_dir, thumbnails_dir, source).map_err(AppError::from)?;
    if is_duplicate && new_img.id == old_id {
        return Ok(ImageReplaceOutcome::SameImage);
    }

    let tx = conn.unchecked_transaction()?;
    // 软删旧图（进回收站可恢复）
    tx.execute(
        "UPDATE images SET is_deleted = 1, deleted_at = strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id = ?1",
        rusqlite::params![old_id],
    )?;
    // 迁移提示词-图像关联（UPDATE OR IGNORE 防新图已有同关联时主键冲突）
    tx.execute(
        "UPDATE OR IGNORE prompt_image_relations SET image_id = ?1 WHERE image_id = ?2",
        rusqlite::params![new_img.id, old_id],
    )?;
    // 迁移图像-标签关联
    tx.execute(
        "UPDATE OR IGNORE image_tag_relations SET image_id = ?1 WHERE image_id = ?2",
        rusqlite::params![new_img.id, old_id],
    )?;
    // 迁移元数据（备注/收藏/安全状态），保留用户手工设置
    tx.execute(
        "UPDATE images
         SET note = (SELECT note FROM images WHERE id = ?1),
             is_favorite = (SELECT is_favorite FROM images WHERE id = ?1),
             is_safe = (SELECT is_safe FROM images WHERE id = ?1)
         WHERE id = ?2",
        rusqlite::params![old_id, new_img.id],
    )?;
    // 更新新图与关联提示词的更新时间
    tx.execute(
        "UPDATE images SET updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id = ?1",
        rusqlite::params![new_img.id],
    )?;
    let related_prompt_ids: Vec<String> = tx
        .prepare("SELECT prompt_id FROM prompt_image_relations WHERE image_id = ?1")?
        .query_map(rusqlite::params![new_img.id], |r| r.get(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if !related_prompt_ids.is_empty() {
        let placeholders = vec!["?"; related_prompt_ids.len()].join(",");
        tx.execute(
            &format!(
                "UPDATE prompts SET updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now')
                 WHERE id IN ({placeholders})"
            ),
            rusqlite::params_from_iter(related_prompt_ids.iter()),
        )?;
    }
    tx.commit()?;

    let image = get_by_id(conn, &new_img.id)?
        .ok_or_else(|| AppError::Message("替换后读取图像失败".into()))?;
    Ok(ImageReplaceOutcome::Replaced {
        image,
        related_prompt_ids,
    })
}

fn row_to_image(row: &rusqlite::Row) -> rusqlite::Result<Image> {
    Ok(Image {
        id: row.get(0)?,
        file_name: row.get(1)?,
        stored_name: row.get(2)?,
        relative_path: row.get(3)?,
        thumbnail_path: row.get(4)?,
        md5: row.get(5)?,
        width: row.get(6)?,
        height: row.get(7)?,
        file_size: row.get(8)?,
        gen_params: row.get(9)?,
        is_deleted: row.get::<_, i64>(10)? != 0,
        deleted_at: row.get(11)?,
        is_favorite: row.get::<_, i64>(12)? != 0,
        is_safe: row.get::<_, i64>(13)? != 0,
        created_at: row.get(14)?,
        updated_at: row.get(15)?,
        note: row.get(16)?,
    })
}

/// 计算文件 MD5（小写 32 位 hex）。
fn file_md5(path: &Path) -> rusqlite::Result<String> {
    use md5::{Digest, Md5};
    let bytes = std::fs::read(path).map_err(io_to_sql)?;
    let digest = Md5::digest(&bytes);
    Ok(digest
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect::<String>())
}

fn io_to_sql(e: std::io::Error) -> rusqlite::Error {
    rusqlite::Error::ToSqlConversionFailure(Box::new(e))
}

/// Unix 秒 → 6 位年月字符串（YYYYMM），使用 Howard Hinnant 的民用日期换算。
fn unix_to_yyyymm(secs: i64) -> String {
    let days = secs.div_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let m = mp + if mp < 10 { 3 } else { -9 };
    let year = if m <= 2 { y + 1 } else { y };
    format!("{:04}{:02}", year, m)
}

/// 将图片关联到已存在的提示词（幂等），返回实际新增的关联数；供新建提示词选择图像时使用。
/// 关联视为双向内容变更，同步提示词与图像两侧的 updated_at。
pub fn relate_image_to_prompt(
    conn: &Connection,
    prompt_id: &str,
    image_id: &str,
) -> rusqlite::Result<usize> {
    let tx = conn.unchecked_transaction()?;
    let inserted = tx.execute(
        "INSERT OR IGNORE INTO prompt_image_relations(prompt_id, image_id) VALUES (?1, ?2)",
        rusqlite::params![prompt_id, image_id],
    )?;
    if inserted > 0 {
        tx.execute(
            "UPDATE prompts SET updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id = ?1",
            rusqlite::params![prompt_id],
        )?;
        tx.execute(
            "UPDATE images SET updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id = ?1",
            rusqlite::params![image_id],
        )?;
    }
    tx.commit()?;
    Ok(inserted)
}

/// 返回单张图像关联的提示词列表（含标题/内容/翻译/备注/标签），供详情页左侧展示。
/// 标签走一次批量查询后按 prompt_id 分组回填，避免每条提示词各查一次的 N+1。
pub fn list_related_prompts(conn: &Connection, image_id: &str) -> Result<Vec<LinkedPrompt>> {
    let mut stmt = conn.prepare(
        "SELECT pr.id, pr.title, pr.content, pr.content_translate, pr.note, pr.is_favorite, pr.is_safe
         FROM prompt_image_relations pir
         JOIN prompts pr ON pr.id = pir.prompt_id
         WHERE pir.image_id = ?1 AND pr.is_deleted = 0
         ORDER BY pr.created_at",
    )?;
    let rows = stmt.query_map(rusqlite::params![image_id], |r| {
        Ok(LinkedPrompt {
            id: r.get(0)?,
            title: r.get(1)?,
            content: r.get(2)?,
            content_translate: r.get(3)?,
            note: r.get(4)?,
            is_favorite: r.get(5)?,
            is_safe: r.get(6)?,
            tags: Vec::new(),
        })
    })?;
    let mut list: Vec<LinkedPrompt> = rows.collect::<Result<_>>()?;
    let ids: Vec<&str> = list.iter().map(|p| p.id.as_str()).collect();
    let mut tags = tags_by_owner(conn, TagDomain::Prompt, &ids)?;
    for p in list.iter_mut() {
        p.tags = tags.remove(&p.id).unwrap_or_default();
    }
    Ok(list)
}

#[cfg(test)]
#[path = "image_service.test.rs"]
mod tests;
