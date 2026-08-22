//! 图像领域服务：承载图像导入（落盘 + 缩略图 + 入库）与查询。
//! 持久化与文件系统细节集中于此，业务层通过命令调用。
//! 存储布局参考 prompt-manager：原图存 images/<年月>，缩略图统一 jpeg 存 thumbnails/。

use image::GenericImageView;
use rusqlite::Connection;
use rusqlite::OptionalExtension;
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize, Clone)]
pub struct Image {
    pub id: i64,
    pub stored_name: String,
    pub relative_path: String,
    pub thumbnail_path: Option<String>,
    pub md5: Option<String>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub file_size: i64,
    pub prompt_id: Option<i64>,
    pub created_at: String,
    pub updated_at: String,
    pub is_deleted: bool,
    pub deleted_at: Option<String>,
}

#[derive(Debug, Serialize, Clone)]
pub struct ImportResult {
    pub image: Image,
    pub is_duplicate: bool,
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
            let img = restore(conn, existing.id)?.ok_or(rusqlite::Error::QueryReturnedNoRows)?;
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
    conn.execute(
        "INSERT INTO images(stored_name, relative_path, thumbnail_path, md5, width, height, file_size)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![stored_name, relative_path, thumb_rel, md5, width, height, file_size],
    )?;
    let id = conn.last_insert_rowid();
    let new_img = get_by_id(conn, id)?.ok_or(rusqlite::Error::QueryReturnedNoRows)?;
    Ok((new_img, false))
}

/// 生成 200×200 居中裁剪的缩略图：先等比缩放覆盖目标尺寸，再裁剪中心。
fn make_center_thumb(
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
        "SELECT id, stored_name, relative_path, thumbnail_path, md5, width, height, file_size, prompt_id, created_at, updated_at, is_deleted, deleted_at
         FROM images WHERE is_deleted = 0 ORDER BY created_at DESC",
    )?;
    let rows = stmt.query_map([], row_to_image)?;
    rows.collect()
}

/// 回收站列表（软删除的图像）。
pub fn list_trashed(conn: &Connection) -> rusqlite::Result<Vec<Image>> {
    let mut stmt = conn.prepare(
        "SELECT id, stored_name, relative_path, thumbnail_path, md5, width, height, file_size, prompt_id, created_at, updated_at, is_deleted, deleted_at
         FROM images WHERE is_deleted = 1 ORDER BY deleted_at DESC",
    )?;
    let rows = stmt.query_map([], row_to_image)?;
    rows.collect()
}

/// 软删除：标记为已删除，保留文件以便恢复。
pub fn soft_delete(conn: &Connection, id: i64) -> rusqlite::Result<Option<Image>> {
    conn.execute(
        "UPDATE images SET is_deleted = 1, deleted_at = datetime('now') WHERE id = ?1",
        rusqlite::params![id],
    )?;
    get_by_id(conn, id)
}

/// 恢复软删除的图像。
pub fn restore(conn: &Connection, id: i64) -> rusqlite::Result<Option<Image>> {
    conn.execute(
        "UPDATE images SET is_deleted = 0, deleted_at = NULL WHERE id = ?1",
        rusqlite::params![id],
    )?;
    get_by_id(conn, id)
}

/// 彻底删除：删除磁盘原图与缩略图并移除记录，不可恢复。
pub fn purge(conn: &Connection, app: &tauri::AppHandle, id: i64) -> rusqlite::Result<()> {
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

fn get_by_id(conn: &Connection, id: i64) -> rusqlite::Result<Option<Image>> {
    let mut stmt = conn.prepare(
        "SELECT id, stored_name, relative_path, thumbnail_path, md5, width, height, file_size, prompt_id, created_at, updated_at, is_deleted, deleted_at
         FROM images WHERE id = ?1",
    )?;
    let mut rows = stmt.query_map(rusqlite::params![id], row_to_image)?;
    rows.next().transpose()
}

fn find_by_md5(conn: &Connection, md5: &str) -> rusqlite::Result<Option<Image>> {
    let mut stmt = conn.prepare(
        "SELECT id, stored_name, relative_path, thumbnail_path, md5, width, height, file_size, prompt_id, created_at, updated_at, is_deleted, deleted_at
         FROM images WHERE md5 = ?1",
    )?;
    let mut rows = stmt.query_map(rusqlite::params![md5], row_to_image)?;
    rows.next().transpose()
}

fn row_to_image(row: &rusqlite::Row) -> rusqlite::Result<Image> {
    Ok(Image {
        id: row.get(0)?,
        stored_name: row.get(1)?,
        relative_path: row.get(2)?,
        thumbnail_path: row.get(3)?,
        md5: row.get(4)?,
        width: row.get(5)?,
        height: row.get(6)?,
        file_size: row.get(7)?,
        prompt_id: row.get(8)?,
        created_at: row.get(9)?,
        updated_at: row.get(10)?,
        is_deleted: row.get::<_, i64>(11)? != 0,
        deleted_at: row.get(12)?,
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

// ---- Tauri commands ----

use tauri::State;

#[tauri::command]
pub fn import_image(
    app: tauri::AppHandle,
    db: State<crate::db::BkDb>,
    path: String,
) -> Result<ImportResult, String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    let (image, is_duplicate) = import(&conn, &app, &path).map_err(|e| e.to_string())?;
    Ok(ImportResult {
        image,
        is_duplicate,
    })
}

#[tauri::command]
pub fn list_images(db: State<crate::db::BkDb>) -> Result<Vec<Image>, String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    list(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_trash(db: State<crate::db::BkDb>) -> Result<Vec<Image>, String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    list_trashed(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_image(db: State<crate::db::BkDb>, id: i64) -> Result<Image, String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    soft_delete(&conn, id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "图像不存在".to_string())
}

#[tauri::command]
pub fn restore_image(db: State<crate::db::BkDb>, id: i64) -> Result<Image, String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    restore(&conn, id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "图像不存在".to_string())
}

#[tauri::command]
pub fn purge_image(app: tauri::AppHandle, db: State<crate::db::BkDb>, id: i64) -> Result<(), String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    purge(&conn, &app, id).map_err(|e| e.to_string())
}

/// 返回指定图像的缩略图磁盘路径，前端配合 convertFileSrc 加载。
#[tauri::command]
pub fn get_thumbnail(
    app: tauri::AppHandle,
    db: State<crate::db::BkDb>,
    id: i64,
) -> Result<String, String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    let rel: Option<String> = conn
        .query_row(
            "SELECT thumbnail_path FROM images WHERE id = ?1",
            rusqlite::params![id],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;

    let Some(rel) = rel else {
        return Err("缩略图不存在".to_string());
    };
    Ok(crate::db::data_dir(&app).join(&rel).to_string_lossy().into_owned())
}