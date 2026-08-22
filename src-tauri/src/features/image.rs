//! 图像领域服务：承载图像导入（落盘 + 缩略图 + 入库）与查询。
//! 持久化与文件系统细节集中于此，业务层通过命令调用。
//! 存储布局参考 prompt-manager：原图存 images/<年月>，缩略图统一 jpeg 存 thumbnails/。

use image::GenericImageView;
use rusqlite::Connection;
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize, Clone)]
pub struct Image {
    pub id: i64,
    pub stored_name: String,
    pub relative_path: String,
    pub thumbnail_path: Option<String>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub file_size: i64,
    pub prompt_id: Option<i64>,
    pub created_at: String,
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
pub fn import(conn: &Connection, app: &tauri::AppHandle, source: &str) -> rusqlite::Result<Image> {
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
        "INSERT INTO images(stored_name, relative_path, thumbnail_path, width, height, file_size)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![stored_name, relative_path, thumb_rel, width, height, file_size],
    )?;
    let id = conn.last_insert_rowid();
    get_by_id(conn, id)?.ok_or(rusqlite::Error::QueryReturnedNoRows)
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
        "SELECT id, stored_name, relative_path, thumbnail_path, width, height, file_size, prompt_id, created_at
         FROM images ORDER BY created_at DESC",
    )?;
    let rows = stmt.query_map([], row_to_image)?;
    rows.collect()
}

fn get_by_id(conn: &Connection, id: i64) -> rusqlite::Result<Option<Image>> {
    let mut stmt = conn.prepare(
        "SELECT id, stored_name, relative_path, thumbnail_path, width, height, file_size, prompt_id, created_at
         FROM images WHERE id = ?1",
    )?;
    let mut rows = stmt.query_map(rusqlite::params![id], row_to_image)?;
    rows.next().transpose()
}

fn row_to_image(row: &rusqlite::Row) -> rusqlite::Result<Image> {
    Ok(Image {
        id: row.get(0)?,
        stored_name: row.get(1)?,
        relative_path: row.get(2)?,
        thumbnail_path: row.get(3)?,
        width: row.get(4)?,
        height: row.get(5)?,
        file_size: row.get(6)?,
        prompt_id: row.get(7)?,
        created_at: row.get(8)?,
    })
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
) -> Result<Image, String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    import(&conn, &app, &path).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_images(db: State<crate::db::BkDb>) -> Result<Vec<Image>, String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    list(&conn).map_err(|e| e.to_string())
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