//! 图像领域服务：承载图像导入（落盘 + 元数据 + 入库）与查询。
//! 持久化与文件系统细节集中于此，业务层通过命令调用。

use rusqlite::Connection;
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize, Clone)]
pub struct Image {
    pub id: i64,
    pub path: String,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub prompt_id: Option<i64>,
    pub created_at: String,
}

/// 允许导入的图片扩展名。
const ALLOWED_EXT: &[&str] = &["png", "jpg", "jpeg", "gif", "webp", "bmp"];

fn ext_ok(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| ALLOWED_EXT.contains(&e.to_lowercase().as_str()))
        .unwrap_or(false)
}

/// 复制源图到应用 images 目录，读取尺寸并入库，返回记录。
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

    // 目标目录：应用数据目录下 images/
    let data_dir = crate::db::user_db_path(app)
        .parent()
        .expect("db path parent")
        .to_path_buf();
    let images_dir = data_dir.join("images");
    std::fs::create_dir_all(&images_dir).map_err(|e| {
        rusqlite::Error::ToSqlConversionFailure(Box::new(std::io::Error::new(
            e.kind(),
            format!("创建图片目录失败: {e}"),
        )))
    })?;

    let ext = source
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("png")
        .to_lowercase();
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let dest = images_dir.join(format!("{stamp}.{ext}"));

    // 读取尺寸（只解析头，轻量）
    let (w, h) = match imagesize::size(&source) {
        Ok(s) => (Some(s.width as i64), Some(s.height as i64)),
        Err(_) => (None, None),
    };

    std::fs::copy(&source, &dest).map_err(|e| {
        rusqlite::Error::ToSqlConversionFailure(Box::new(std::io::Error::new(
            e.kind(),
            format!("复制图片失败: {e}"),
        )))
    })?;

    conn.execute(
        "INSERT INTO images(path, width, height) VALUES (?1, ?2, ?3)",
        rusqlite::params![dest.to_string_lossy(), w, h],
    )?;
    let id = conn.last_insert_rowid();
    get_by_id(conn, id)?.ok_or_else(|| {
        rusqlite::Error::QueryReturnedNoRows
    })
}

pub fn list(conn: &Connection) -> rusqlite::Result<Vec<Image>> {
    let mut stmt = conn.prepare(
        "SELECT id, path, width, height, prompt_id, created_at FROM images ORDER BY created_at DESC",
    )?;
    let rows = stmt.query_map([], row_to_image)?;
    rows.collect()
}

fn get_by_id(conn: &Connection, id: i64) -> rusqlite::Result<Option<Image>> {
    let mut stmt = conn.prepare(
        "SELECT id, path, width, height, prompt_id, created_at FROM images WHERE id = ?1",
    )?;
    let mut rows = stmt.query_map(rusqlite::params![id], row_to_image)?;
    rows.next().transpose()
}

fn row_to_image(row: &rusqlite::Row) -> rusqlite::Result<Image> {
    Ok(Image {
        id: row.get(0)?,
        path: row.get(1)?,
        width: row.get(2)?,
        height: row.get(3)?,
        prompt_id: row.get(4)?,
        created_at: row.get(5)?,
    })
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