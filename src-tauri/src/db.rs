//! 数据库连接管理与 schema 初始化。
//! 持久化细节集中在基础设施层，业务层通过 repository 接口访问。

use rusqlite::Connection;
use std::path::PathBuf;

/// 应用持有的数据库连接（单连接 + Mutex），通过 Tauri managed state 注入。
pub struct BkDb(pub std::sync::Mutex<Connection>);

/// 打开（必要时创建）数据库并执行 DDL。
/// 表名与字段名与 prompt-manager 完全一致，便于后续数据导入；
/// 时间列沿用本项目的 ISO 8601 UTC 约定（详见项目 memory）。
pub fn init(path: PathBuf) -> rusqlite::Result<BkDb> {
    let conn = Connection::open(&path)?;
    conn.execute_batch(
        r#"
        PRAGMA journal_mode = WAL;
        PRAGMA foreign_keys = ON;

        -- 提示词表
        CREATE TABLE IF NOT EXISTS prompts (
            id TEXT PRIMARY KEY,
            title TEXT NOT NULL,
            content TEXT NOT NULL,
            content_translate TEXT DEFAULT '',
            created_at TEXT DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
            updated_at TEXT DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
            is_deleted INTEGER DEFAULT 0,
            deleted_at TEXT,
            is_favorite INTEGER DEFAULT 0,
            is_safe INTEGER DEFAULT 1,
            note TEXT DEFAULT ''
        );

        -- 图像表
        CREATE TABLE IF NOT EXISTS images (
            id TEXT PRIMARY KEY,
            file_name TEXT NOT NULL,
            stored_name TEXT NOT NULL,
            relative_path TEXT NOT NULL,
            thumbnail_path TEXT,
            md5 TEXT UNIQUE,
            width INTEGER,
            height INTEGER,
            file_size INTEGER DEFAULT 0,
            gen_params TEXT DEFAULT '{}',
            is_deleted INTEGER DEFAULT 0,
            deleted_at TEXT,
            is_favorite INTEGER DEFAULT 0,
            is_safe INTEGER DEFAULT 1,
            created_at TEXT DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
            updated_at TEXT DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
            note TEXT DEFAULT ''
        );

        -- 提示词标签组表
        CREATE TABLE IF NOT EXISTS prompt_tag_groups (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL UNIQUE,
            sort_order INTEGER DEFAULT 0,
            created_at TEXT DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
            updated_at TEXT DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
        );

        -- 提示词标签表
        CREATE TABLE IF NOT EXISTS prompt_tags (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT UNIQUE NOT NULL,
            group_id INTEGER,
            created_at TEXT DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
            updated_at TEXT DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
            FOREIGN KEY (group_id) REFERENCES prompt_tag_groups(id) ON DELETE SET NULL
        );

        -- 提示词-标签关联表
        CREATE TABLE IF NOT EXISTS prompt_tag_relations (
            prompt_id TEXT,
            tag_id INTEGER,
            PRIMARY KEY (prompt_id, tag_id),
            FOREIGN KEY (prompt_id) REFERENCES prompts(id) ON DELETE CASCADE,
            FOREIGN KEY (tag_id) REFERENCES prompt_tags(id) ON DELETE CASCADE
        );

        -- 提示词-图像关联表
        CREATE TABLE IF NOT EXISTS prompt_image_relations (
            prompt_id TEXT,
            image_id TEXT,
            sort_order INTEGER DEFAULT 0,
            PRIMARY KEY (prompt_id, image_id),
            FOREIGN KEY (prompt_id) REFERENCES prompts(id) ON DELETE CASCADE,
            FOREIGN KEY (image_id) REFERENCES images(id) ON DELETE CASCADE
        );

        -- 图像标签组表
        CREATE TABLE IF NOT EXISTS image_tag_groups (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL UNIQUE,
            sort_order INTEGER DEFAULT 0,
            created_at TEXT DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
            updated_at TEXT DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
        );

        -- 图像标签表
        CREATE TABLE IF NOT EXISTS image_tags (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT UNIQUE NOT NULL,
            group_id INTEGER,
            created_at TEXT DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
            updated_at TEXT DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
            FOREIGN KEY (group_id) REFERENCES image_tag_groups(id) ON DELETE SET NULL
        );

        -- 图像-标签关联表
        CREATE TABLE IF NOT EXISTS image_tag_relations (
            image_id TEXT,
            tag_id INTEGER,
            PRIMARY KEY (image_id, tag_id),
            FOREIGN KEY (image_id) REFERENCES images(id) ON DELETE CASCADE,
            FOREIGN KEY (tag_id) REFERENCES image_tags(id) ON DELETE CASCADE
        );

        -- 数据库版本表
        CREATE TABLE IF NOT EXISTS db_version (
            version INTEGER PRIMARY KEY,
            applied_at TEXT DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
        );
        "#,
    )?;
    Ok(BkDb(std::sync::Mutex::new(conn)))
}

/// 生成与 prompt-manager 同格式的文本主键："{prefix}_{毫秒时间戳}_{随机4位hex}"。
/// 导入时保留 prompt-manager 原有 id；此处仅用于本应用新建记录。
pub fn gen_id(prefix: &str) -> String {
    use md5::{Digest, Md5};
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let digest = Md5::digest(format!("{nanos}").as_bytes());
    let rand_hex: String = digest
        .iter()
        .take(2)
        .map(|b| format!("{:02x}", b))
        .collect();
    format!("{prefix}_{nanos}_{}", rand_hex)
}

/// 提示词主键前缀。
pub const PROMPT_ID_PREFIX: &str = "pmt";
/// 图像主键前缀。
pub const IMAGE_ID_PREFIX: &str = "img";

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

/// 缩略图存储目录（数据目录基准下）。
pub fn thumbnails_dir(app: &tauri::AppHandle) -> PathBuf {
    base_data_dir(app).join("thumbnails")
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