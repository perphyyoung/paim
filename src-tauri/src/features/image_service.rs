//! 图像领域服务：承载图像存储（落盘 + 缩略图 + 入库）与查询的业务规则。
//! 领域层感知 app（用于定位数据目录）与连接访问数据，但不直接面向 IPC。
//! 命令层见 `features::image`。

use crate::features::prompt_service;
use image::GenericImageView;
use rusqlite::{Connection, OptionalExtension, Result};
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize, Clone)]
pub struct Image {
    pub id: String,
    pub file_name: String,
    pub stored_name: String,
    pub relative_path: String,
    pub thumbnail_path: Option<String>,
    pub md5: Option<String>,
    pub width: Option<i64>,
    pub height: Option<i64>,
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

#[derive(Debug, Serialize, Clone)]
pub struct ImportResult {
    pub image: Image,
    pub is_duplicate: bool,
}

#[derive(Debug, Serialize, Clone)]
pub struct ImportError {
    pub path: String,
    pub message: String,
}

#[derive(Debug, Serialize, Clone)]
pub struct ImportBatchResult {
    pub results: Vec<ImportResult>,
    pub errors: Vec<ImportError>,
}

/// 图像标签（关联表 join image_tags 的返回对象）。
#[derive(Debug, Serialize, Clone)]
pub struct ImageTag {
    pub id: i64,
    pub name: String,
    /// 所属标签组，未分组为 None；其它命令不填时默认为 None
    #[serde(default)]
    pub group_id: Option<i64>,
}

#[derive(Debug, Serialize, Clone)]
pub struct LinkedPrompt {
    pub id: String,
    pub title: String,
    pub content: String,
    pub content_translate: String,
    pub note: String,
    pub tags: Vec<String>,
}

/// 缩略图尺寸（宽=高=200，居中裁剪）。
const THUMB_SIZE: u32 = 200;

/// 允许导入的图片扩展名。
const ALLOWED_EXT: &[&str] = &["png", "jpg", "jpeg", "gif", "webp", "bmp"];

fn ext_ok(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| ALLOWED_EXT.contains(&e.to_lowercase().as_str()))
        .unwrap_or(false)
}

/// 复制源图到 images/<年月>/，生成 200×200 居中裁剪的 jpeg 缩略图，入库，返回记录。
/// 返回 `(记录, 是否重复)`：重复时复用已存在记录且不落新文件。
pub fn import(
    conn: &Connection,
    app: &tauri::AppHandle,
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

    let images_dir = crate::db::images_dir(app);
    let thumbnails_dir = crate::db::thumbnails_dir(app);
    std::fs::create_dir_all(&images_dir).map_err(io_to_sql)?;
    std::fs::create_dir_all(&thumbnails_dir).map_err(io_to_sql)?;

    let md = std::fs::metadata(&source).map_err(io_to_sql)?;
    let file_size = md.len() as i64;

    // 6 位年月子目录：images/YYYYMM/
    let yyyymm = unix_to_yyyymm(std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64);
    let month_dir = images_dir.join(&yyyymm);
    std::fs::create_dir_all(&month_dir).map_err(io_to_sql)?;

    let ext = source
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("png")
        .to_lowercase();
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let stored_name = format!("{stamp}.{ext}");
    let dest = month_dir.join(&stored_name);
    std::fs::copy(&source, &dest).map_err(io_to_sql)?;

    // 解码原图（同时拿到尺寸），生成居中裁剪的方图缩略图
    let (width, height, thumb_rel) = match image::open(&source) {
        Ok(img) => {
            let (w, h) = img.dimensions();
            match make_center_thumb(&img) {
                Ok(thumb) => {
                    let thumb_name = format!("thumb_{stamp}.jpg");
                    let thumb_abs = thumbnails_dir.join(&thumb_name);
                    thumb.save(&thumb_abs).map_err(|e| {
                        rusqlite::Error::ToSqlConversionFailure(Box::new(
                            std::io::Error::new(
                                std::io::ErrorKind::Other,
                                format!("生成缩略图失败: {e}"),
                            ),
                        ))
                    })?;
                    (
                        Some(w as i64),
                        Some(h as i64),
                        Some(format!("thumbnails/{}", thumb_name)),
                    )
                }
                Err(_) => (Some(w as i64), Some(h as i64), None),
            }
        }
        Err(_) => (None, None, None),
    };

    let relative_path = format!("images/{yyyymm}/{stored_name}");
    let id = crate::db::gen_id(crate::db::IMAGE_ID_PREFIX);
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

/// 生成 200×200 居中裁剪的缩略图：先等比缩放覆盖目标尺寸，再裁剪中心。
pub(crate) fn make_center_thumb(
    img: &image::DynamicImage,
) -> Result<image::DynamicImage, image::ImageError> {
    let scaled = img.thumbnail(THUMB_SIZE, THUMB_SIZE);
    let (w, h) = scaled.dimensions();
    let x = w.saturating_sub(THUMB_SIZE) / 2;
    let y = h.saturating_sub(THUMB_SIZE) / 2;
    Ok(scaled.crop_imm(x, y, THUMB_SIZE.min(w), THUMB_SIZE.min(h)))
}

pub fn list(conn: &Connection) -> rusqlite::Result<Vec<Image>> {
    let mut stmt = conn.prepare(
        "SELECT id, file_name, stored_name, relative_path, thumbnail_path, md5, width, height, file_size, gen_params, is_deleted, deleted_at, is_favorite, is_safe, created_at, updated_at, note
         FROM images WHERE is_deleted = 0 ORDER BY created_at DESC",
    )?;
    let rows = stmt.query_map([], row_to_image)?;
    rows.collect()
}

/// 回收站列表（软删除的图像）。
pub fn list_trashed(conn: &Connection) -> rusqlite::Result<Vec<Image>> {
    let mut stmt = conn.prepare(
        "SELECT id, file_name, stored_name, relative_path, thumbnail_path, md5, width, height, file_size, gen_params, is_deleted, deleted_at, is_favorite, is_safe, created_at, updated_at, note
         FROM images WHERE is_deleted = 1 ORDER BY deleted_at DESC",
    )?;
    let rows = stmt.query_map([], row_to_image)?;
    rows.collect()
}

/// 软删除：标记为已删除，保留文件以便恢复。
pub fn soft_delete(conn: &Connection, id: &str) -> rusqlite::Result<Option<Image>> {
    conn.execute(
        "UPDATE images SET is_deleted = 1, deleted_at = strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id = ?1",
        rusqlite::params![id],
    )?;
    get_by_id(conn, id)
}

/// 恢复软删除的图像。
pub fn restore(conn: &Connection, id: &str) -> rusqlite::Result<Option<Image>> {
    conn.execute(
        "UPDATE images SET is_deleted = 0, deleted_at = NULL WHERE id = ?1",
        rusqlite::params![id],
    )?;
    get_by_id(conn, id)
}

/// 更新图像详情字段（文件名、备注、收藏、安全评级）。仅更新传入非默认值的字段。
pub fn update_detail(
    conn: &Connection,
    id: &str,
    file_name: Option<&str>,
    note: Option<&str>,
    is_favorite: Option<bool>,
    is_safe: Option<bool>,
) -> rusqlite::Result<Option<Image>> {
    let mut sql = String::from("UPDATE images SET updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now')");
    let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
    if let Some(v) = file_name {
        if !v.trim().is_empty() {
            sql.push_str(", file_name = ?");
            params.push(Box::new(v.trim().to_string()));
        }
    }
    if let Some(v) = note {
        sql.push_str(", note = ?");
        params.push(Box::new(v.to_string()));
    }
    if let Some(v) = is_favorite {
        sql.push_str(", is_favorite = ?");
        params.push(Box::new(if v { 1 } else { 0 }));
    }
    if let Some(v) = is_safe {
        sql.push_str(", is_safe = ?");
        params.push(Box::new(if v { 1 } else { 0 }));
    }
    sql.push_str(" WHERE id = ?");
    params.push(Box::new(id.to_string()));

    let mut stmt = conn.prepare(&sql)?;
    let param_refs: Vec<&dyn rusqlite::types::ToSql> =
        params.iter().map(|b| b.as_ref()).collect();
    stmt.execute(param_refs.as_slice())?;
    get_by_id(conn, id)
}

/// 彻底删除：删除磁盘原图与缩略图并移除记录，不可恢复。
pub fn purge(conn: &Connection, app: &tauri::AppHandle, id: &str) -> rusqlite::Result<()> {
    let row: Option<(Option<String>, Option<String>)> = conn
        .query_row(
            "SELECT relative_path, thumbnail_path FROM images WHERE id = ?1",
            rusqlite::params![id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;

    if let Some((rel, thumb)) = row {
        let data_dir = crate::db::data_dir(app);
        if let Some(rel) = rel {
            let _ = std::fs::remove_file(data_dir.join(rel));
        }
        if let Some(thumb) = thumb {
            let _ = std::fs::remove_file(data_dir.join(thumb));
        }
        conn.execute("DELETE FROM images WHERE id = ?1", rusqlite::params![id])?;
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

/// 直接新建一条提示词并与该图片建立关联（幂等）。
pub(crate) fn relate_prompt(conn: &Connection, image_id: &str, content: &str) -> rusqlite::Result<()> {
    let prompt_id = prompt_service::create(conn, content, None)?.id;
    conn.execute(
        "INSERT OR IGNORE INTO prompt_image_relations(prompt_id, image_id) VALUES (?1, ?2)",
        rusqlite::params![prompt_id, image_id],
    )?;
    Ok(())
}

/// 将图片关联到已存在的提示词（幂等），返回实际新增的关联数；供新建提示词选择图像时使用。
pub fn relate_image_to_prompt(
    conn: &Connection,
    prompt_id: &str,
    image_id: &str,
) -> rusqlite::Result<usize> {
    conn.execute(
        "INSERT OR IGNORE INTO prompt_image_relations(prompt_id, image_id) VALUES (?1, ?2)",
        rusqlite::params![prompt_id, image_id],
    )
}