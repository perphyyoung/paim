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

/// 解析数据库文件路径（应用数据目录下）。
pub fn user_db_path(app: &tauri::AppHandle) -> PathBuf {
    use tauri::Manager;
    app.path()
        .app_data_dir()
        .expect("failed to resolve app data dir")
        .join("paim.db")
}