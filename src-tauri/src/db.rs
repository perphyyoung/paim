//! 数据库连接管理与 schema 初始化。
//! 持久化细节集中在基础设施层，业务层通过 repository 接口访问。

use rusqlite::Connection;
use std::path::PathBuf;

/// 应用持有的数据库连接（单连接 + Mutex），通过 Tauri managed state 注入。
pub struct BkDb(pub std::sync::Mutex<Connection>);

/// 打开（必要时创建）数据库并执行迁移。
pub fn init(path: PathBuf) -> rusqlite::Result<BkDb> {
    let conn = Connection::open(&path)?;
    conn.execute_batch(
        r#"
        PRAGMA journal_mode = WAL;
        PRAGMA foreign_keys = ON;

        CREATE TABLE IF NOT EXISTS tags (
            id   INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL UNIQUE,
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE TABLE IF NOT EXISTS prompts (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            content     TEXT NOT NULL,
            title       TEXT,
            created_at  TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE TABLE IF NOT EXISTS images (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            path        TEXT NOT NULL UNIQUE,
            width       INTEGER,
            height      INTEGER,
            prompt_id   INTEGER REFERENCES prompts(id) ON DELETE SET NULL,
            created_at  TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE TABLE IF NOT EXISTS prompt_tags (
            prompt_id INTEGER NOT NULL REFERENCES prompts(id) ON DELETE CASCADE,
            tag_id    INTEGER NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
            PRIMARY KEY (prompt_id, tag_id)
        );

        CREATE TABLE IF NOT EXISTS image_tags (
            image_id INTEGER NOT NULL REFERENCES images(id) ON DELETE CASCADE,
            tag_id   INTEGER NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
            PRIMARY KEY (image_id, tag_id)
        );
        "#,
    )?;
    Ok(BkDb(std::sync::Mutex::new(conn)))
}

/// 数据目录基准：
/// - 开发环境（debug）使用项目根目录下的 paim-data（从进程启动目录定位，
///   需从项目根启动 `cargo tauri dev`）；
/// - 部署环境使用 Windows 默认的应用数据目录。
fn base_data_dir(app: &tauri::AppHandle) -> PathBuf {
    if cfg!(debug_assertions) {
        std::env::current_dir()
            .map(|d| d.join("paim-data"))
            .unwrap_or_else(|_| default_data_dir(app))
    } else {
        default_data_dir(app)
    }
}

/// 数据目录（images 与 paim.db 的父目录）。
pub fn data_dir(app: &tauri::AppHandle) -> PathBuf {
    base_data_dir(app)
}

fn default_data_dir(app: &tauri::AppHandle) -> PathBuf {
    use tauri::Manager;
    app.path()
        .app_data_dir()
        .expect("failed to resolve app data dir")
}

/// 解析数据库文件路径（数据目录基准下）。
pub fn user_db_path(app: &tauri::AppHandle) -> PathBuf {
    base_data_dir(app).join("paim.db")
}

/// 图像存储目录（数据目录基准下）。
pub fn images_dir(app: &tauri::AppHandle) -> PathBuf {
    base_data_dir(app).join("images")
}

// ---- Tauri commands ----

#[tauri::command]
pub fn get_data_dir(app: tauri::AppHandle) -> String {
    data_dir(&app).to_string_lossy().into_owned()
}

#[tauri::command]
pub fn open_data_dir(app: tauri::AppHandle) -> Result<(), String> {
    let dir = data_dir(&app);
    std::process::Command::new("explorer")
        .arg(&dir)
        .spawn()
        .map_err(|e| format!("打开目录失败: {e}"))?;
    Ok(())
}