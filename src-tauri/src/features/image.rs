//! 图像领域服务：承载图像导入（落盘 + 缩略图 + 入库）与查询。
//! 持久化与文件系统细节集中于此，业务层通过命令调用。
//! 存储布局参考 prompt-manager：原图存 images/<年月>，缩略图统一 jpeg 存 thumbnails/。

use crate::features::prompt::service as prompt_service;
use image::GenericImageView;
use rusqlite::Connection;
use rusqlite::OptionalExtension;
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

// ---- Tauri commands ----

use tauri::State;

/// 直接新建一条提示词并与该图片建立关联（幂等）。
fn relate_prompt(conn: &Connection, image_id: &str, content: &str) -> rusqlite::Result<()> {
    let prompt_id = prompt_service::create(conn, content, None)?.id;
    conn.execute(
        "INSERT OR IGNORE INTO prompt_image_relations(prompt_id, image_id) VALUES (?1, ?2)",
        rusqlite::params![prompt_id, image_id],
    )?;
    Ok(())
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

/// 批量导入图像；prompt 非空时，<b>同一内容提示词</b>会应用到本次导入的每一张图。
#[tauri::command]
pub fn import_images(
    app: tauri::AppHandle,
    db: State<crate::db::BkDb>,
    paths: Vec<String>,
    prompt: Option<String>,
) -> Result<ImportBatchResult, String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    let prompt = prompt
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let mut results = Vec::new();
    let mut errors = Vec::new();
    for path in &paths {
        match import(&conn, &app, path) {
            Ok((image, is_duplicate)) => {
                if let Some(content) = &prompt {
                    if let Err(e) = relate_prompt(&conn, &image.id, content) {
                        errors.push(ImportError {
                            path: path.clone(),
                            message: format!("关联提示词失败: {e}"),
                        });
                    }
                }
                results.push(ImportResult { image, is_duplicate });
            }
            Err(e) => errors.push(ImportError {
                path: path.clone(),
                message: e.to_string(),
            }),
        }
    }
    Ok(ImportBatchResult { results, errors })
}

/// 为导入弹窗提供源图预览缩略图：解码源图生成居中缩略图，写入 data 目录（已在 asset scope 内）。
#[tauri::command]
pub fn get_source_thumbnail(app: tauri::AppHandle, source: String) -> Result<String, String> {
    let src = PathBuf::from(&source);
    if !src.is_file() {
        return Err("源文件不存在".to_string());
    }
    let img = image::open(&src).map_err(|e| format!("无法读取图像: {e}"))?;
    let thumb = make_center_thumb(&img).map_err(|e| format!("生成缩略图失败: {e}"))?;

    let prev_dir = crate::db::data_dir(&app).join("preview");
    std::fs::create_dir_all(&prev_dir).map_err(|e| e.to_string())?;
    // 以源文件路径哈希命名，重复选择复用
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    use std::hash::Hasher;
    hasher.write(source.as_bytes());
    let name = format!("pre_{:016x}.jpg", hasher.finish());
    let dest = prev_dir.join(&name);

    if !dest.exists() {
        thumb
            .save(&dest)
            .map_err(|e| format!("保存预览失败: {e}"))?;
    }
    Ok(dest.to_string_lossy().to_string())
}

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
pub fn delete_image(db: State<crate::db::BkDb>, id: String) -> Result<Image, String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    soft_delete(&conn, &id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "图像不存在".to_string())
}

#[tauri::command]
pub fn restore_image(db: State<crate::db::BkDb>, id: String) -> Result<Image, String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    restore(&conn, &id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "图像不存在".to_string())
}

#[tauri::command]
pub fn purge_image(app: tauri::AppHandle, db: State<crate::db::BkDb>, id: String) -> Result<(), String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    purge(&conn, &app, &id).map_err(|e| e.to_string())
}

/// 返回指定图像的缩略图磁盘路径，前端配合 convertFileSrc 加载。
#[tauri::command]
pub fn get_thumbnail(
    app: tauri::AppHandle,
    db: State<crate::db::BkDb>,
    id: String,
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

/// 返回单张图像详情。
#[tauri::command]
pub fn get_image_detail(db: State<crate::db::BkDb>, id: String) -> Result<Image, String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    get_by_id(&conn, &id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "图像不存在".to_string())
}

/// 返回图像原图磁盘路径，前端配合 convertFileSrc 加载（详情页大图使用）。
#[tauri::command]
pub fn get_image_src(
    app: tauri::AppHandle,
    db: State<crate::db::BkDb>,
    id: String,
) -> Result<String, String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    let rel: Option<String> = conn
        .query_row(
            "SELECT relative_path FROM images WHERE id = ?1",
            rusqlite::params![id],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;

    let Some(rel) = rel else {
        return Err("原图不存在".to_string());
    };
    Ok(crate::db::data_dir(&app).join(&rel).to_string_lossy().into_owned())
}

/// 更新图像详情字段（文件名、备注、收藏、安全评级）。
#[tauri::command]
pub fn update_image_detail(
    db: State<crate::db::BkDb>,
    id: String,
    file_name: Option<String>,
    note: Option<String>,
    is_favorite: Option<bool>,
    is_safe: Option<bool>,
) -> Result<Image, String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    update_detail(
        &conn,
        &id,
        file_name.as_deref(),
        note.as_deref(),
        is_favorite,
        is_safe,
    )
    .map_err(|e| e.to_string())?
    .ok_or_else(|| "图像不存在".to_string())
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

/// 返回图像的标签列表。
#[tauri::command]
pub fn get_image_tags(db: State<crate::db::BkDb>, id: String) -> Result<Vec<ImageTag>, String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(
            "SELECT it.id, it.name
             FROM image_tags it
             JOIN image_tag_relations itr ON itr.tag_id = it.id
             WHERE itr.image_id = ?1
             ORDER BY it.name",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(rusqlite::params![id], |r| {
            Ok(ImageTag {
                id: r.get(0)?,
                name: r.get(1)?,
                group_id: None,
            })
        })
        .map_err(|e| e.to_string())?;
    let mut tags = Vec::new();
    for row in rows {
        tags.push(row.map_err(|e| e.to_string())?);
    }
    Ok(tags)
}

/// 为图像添加多个标签：标签不存在则创建，关联存在则忽略，并更新图像的 updated_at。
#[tauri::command]
pub fn add_image_tags(
    db: State<crate::db::BkDb>,
    id: String,
    names: Vec<String>,
) -> Result<Vec<ImageTag>, String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;

    let mut result = Vec::new();
    for raw in names {
        let name = raw.trim();
        if name.is_empty() {
            continue;
        }
        // 获取或创建标签
        let tag_id: i64 = match tx
            .query_row(
                "SELECT id FROM image_tags WHERE name = ?1",
                rusqlite::params![name],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?
        {
            Some(tid) => tid,
            None => {
                tx.execute(
                    "INSERT INTO image_tags(name) VALUES (?1)",
                    rusqlite::params![name],
                )
                .map_err(|e| e.to_string())?;
                tx.last_insert_rowid()
            }
        };
        // 建立关联（重复时因主键冲突报错，忽略）
        let _ = tx.execute(
            "INSERT OR IGNORE INTO image_tag_relations(image_id, tag_id) VALUES (?1, ?2)",
            rusqlite::params![id, tag_id],
        );
        result.push(ImageTag { id: tag_id, name: name.to_string(), group_id: None });
    }

    tx.execute(
        "UPDATE images SET updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id = ?1",
        rusqlite::params![id],
    )
    .map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(result)
}

/// 移除图像的一个标签关联。
#[tauri::command]
pub fn remove_image_tag(
    db: State<crate::db::BkDb>,
    id: String,
    tag_id: i64,
) -> Result<(), String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    conn.execute(
        "DELETE FROM image_tag_relations WHERE image_id = ?1 AND tag_id = ?2",
        rusqlite::params![id, tag_id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// 返回全部图像标签（供标签筛选区渲染），按名称排序。
#[tauri::command]
pub fn list_all_image_tags(db: State<crate::db::BkDb>) -> Result<Vec<ImageTag>, String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare("SELECT id, name, group_id FROM image_tags ORDER BY name")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| {
            Ok(ImageTag {
                id: r.get(0)?,
                name: r.get(1)?,
                group_id: r.get(2)?,
            })
        })
        .map_err(|e| e.to_string())?;
    let mut tags = Vec::new();
    for row in rows {
        tags.push(row.map_err(|e| e.to_string())?);
    }
    Ok(tags)
}

/// 返回非删除图像到其标签名的映射：{imageId: [tagName,...]}，供前端内存过滤。
#[tauri::command]
pub fn get_image_tags_map(
    db: State<crate::db::BkDb>,
) -> Result<std::collections::HashMap<String, Vec<String>>, String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(
            "SELECT img.id, it.name
             FROM images img
             JOIN image_tag_relations itr ON itr.image_id = img.id
             JOIN image_tags it ON it.id = itr.tag_id
             WHERE img.is_deleted = 0
             ORDER BY it.name",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
        .map_err(|e| e.to_string())?;
    let mut map: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    for row in rows {
        let (img_id, tag_name) = row.map_err(|e| e.to_string())?;
        map.entry(img_id).or_default().push(tag_name);
    }
    Ok(map)
}

/// 返回非删除图像到其关联提示词内容的映射：{imageId: [content,...]}，供卡片 row2 显示。
#[tauri::command]
pub fn get_image_prompts_map(
    db: State<crate::db::BkDb>,
) -> Result<std::collections::HashMap<String, Vec<String>>, String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(
            "SELECT img.id, pr.content
             FROM images img
             JOIN prompt_image_relations pir ON pir.image_id = img.id
             JOIN prompts pr ON pr.id = pir.prompt_id
             WHERE img.is_deleted = 0 AND pr.is_deleted = 0
             ORDER BY pr.created_at",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
        .map_err(|e| e.to_string())?;
    let mut map: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    for row in rows {
        let (img_id, content) = row.map_err(|e| e.to_string())?;
        map.entry(img_id).or_default().push(content);
    }
    Ok(map)
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

/// 返回单张图像关联的提示词列表（含标题/内容/翻译/备注/标签），供详情页左侧展示。
#[tauri::command]
pub fn get_image_related_prompts(
    db: State<crate::db::BkDb>,
    id: String,
) -> Result<Vec<LinkedPrompt>, String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(
            "SELECT pr.id, pr.title, pr.content, pr.content_translate, pr.note
             FROM prompt_image_relations pir
             JOIN prompts pr ON pr.id = pir.prompt_id
             WHERE pir.image_id = ?1 AND pr.is_deleted = 0
             ORDER BY pr.created_at",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(rusqlite::params![id], |r| {
            Ok(LinkedPrompt {
                id: r.get(0)?,
                title: r.get(1)?,
                content: r.get(2)?,
                content_translate: r.get(3)?,
                note: r.get(4)?,
                tags: Vec::new(),
            })
        })
        .map_err(|e| e.to_string())?;
    let mut list: Vec<LinkedPrompt> = rows.collect::<Result<_, _>>().map_err(|e| e.to_string())?;
    // 补充分组查询每条提示词的标签
    for p in list.iter_mut() {
        let mut t = conn
            .prepare(
                "SELECT pt.name
                 FROM prompt_tag_relations ptr
                 JOIN prompt_tags pt ON pt.id = ptr.tag_id
                 WHERE ptr.prompt_id = ?1
                 ORDER BY pt.name",
            )
            .map_err(|e| e.to_string())?;
        let names = t
            .query_map(rusqlite::params![p.id], |r| r.get::<_, String>(0))
            .map_err(|e| e.to_string())?;
        p.tags = names.collect::<Result<_, _>>().map_err(|e| e.to_string())?;
    }
    Ok(list)
}