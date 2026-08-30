//! 数据库连接管理与 schema 初始化。
//! 持久化细节集中在基础设施层，业务层通过 repository 接口访问。

use rusqlite::Connection;
use std::path::{Path, PathBuf};

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

        -- 索引定义与 prompt-manager 保持一致,保证导入 pm 数据后查询性能对齐
        CREATE INDEX IF NOT EXISTS idx_prompts_updated_at ON prompts(updated_at DESC);
        CREATE INDEX IF NOT EXISTS idx_prompts_created_at ON prompts(created_at DESC);
        CREATE INDEX IF NOT EXISTS idx_prompts_is_deleted ON prompts(is_deleted);
        CREATE INDEX IF NOT EXISTS idx_prompts_is_favorite ON prompts(is_favorite);
        CREATE INDEX IF NOT EXISTS idx_prompts_is_safe ON prompts(is_safe);
        CREATE INDEX IF NOT EXISTS idx_prompts_deleted_updated ON prompts(is_deleted, updated_at DESC);
        CREATE INDEX IF NOT EXISTS idx_prompts_title_deleted ON prompts(title, is_deleted);
        CREATE INDEX IF NOT EXISTS idx_images_updated_at ON images(updated_at DESC);
        CREATE INDEX IF NOT EXISTS idx_images_created_at ON images(created_at DESC);
        CREATE INDEX IF NOT EXISTS idx_images_is_deleted ON images(is_deleted);
        CREATE INDEX IF NOT EXISTS idx_images_is_favorite ON images(is_favorite);
        CREATE INDEX IF NOT EXISTS idx_images_is_safe ON images(is_safe);
        CREATE INDEX IF NOT EXISTS idx_images_md5 ON images(md5);
        CREATE INDEX IF NOT EXISTS idx_images_deleted_updated ON images(is_deleted, updated_at DESC);
        CREATE INDEX IF NOT EXISTS idx_prompt_image_relations_prompt_sort ON prompt_image_relations(prompt_id, sort_order ASC);
        CREATE INDEX IF NOT EXISTS idx_prompt_image_relations_image_id ON prompt_image_relations(image_id);
        CREATE INDEX IF NOT EXISTS idx_prompt_tag_relations_tag_id ON prompt_tag_relations(tag_id);
        CREATE INDEX IF NOT EXISTS idx_image_tag_relations_image_id ON image_tag_relations(image_id);
        CREATE INDEX IF NOT EXISTS idx_image_tag_relations_tag_id ON image_tag_relations(tag_id);
        CREATE INDEX IF NOT EXISTS idx_prompt_tags_group_id ON prompt_tags(group_id);
        CREATE INDEX IF NOT EXISTS idx_image_tags_group_id ON image_tags(group_id);
        CREATE INDEX IF NOT EXISTS idx_prompts_active_updated ON prompts(updated_at DESC) WHERE is_deleted = 0;
        CREATE INDEX IF NOT EXISTS idx_images_active_updated ON images(updated_at DESC) WHERE is_deleted = 0;
        CREATE INDEX IF NOT EXISTS idx_prompts_active_favorite ON prompts(updated_at DESC) WHERE is_deleted = 0 AND is_favorite = 1;
        CREATE INDEX IF NOT EXISTS idx_images_active_favorite ON images(updated_at DESC) WHERE is_deleted = 0 AND is_favorite = 1;
        "#,
    )?;
    Ok(BkDb(std::sync::Mutex::new(conn)))
}

/// 生成与 prompt-manager 同格式的文本主键："{prefix}_{YYYYMMDDHHmmss}_{随机5位base36}"。
/// 导入 pm 备份时保留其原有 id；此处仅用于本应用新建记录。
pub fn gen_id(prefix: &str) -> String {
    use md5::{Digest, Md5};
    const BASE36: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let digest = Md5::digest(format!("{nanos}").as_bytes());
    let rand_part: String = digest
        .iter()
        .take(5)
        .map(|b| BASE36[(*b as usize) % BASE36.len()] as char)
        .collect();
    let stamp = chrono::Local::now().format("%Y%m%d%H%M%S");
    format!("{prefix}_{stamp}_{rand_part}")
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

/// 数据集切换防呆的核心检查：激活数据目录不存在时，
/// 扫描其父目录下「<数据目录名>.」前缀的兄弟目录，返回发现的备用数据集目录名。
/// 非空即视为切换未完成，调用方应提示用户而不是静默创建空库。
fn pending_switch_datasets_at(base: &Path) -> Vec<String> {
    if base.exists() {
        return Vec::new();
    }
    let Some(name) = base.file_name().and_then(|n| n.to_str()) else {
        return Vec::new();
    };
    let Some(parent) = base.parent() else {
        return Vec::new();
    };
    let prefix = format!("{name}.");
    let Ok(entries) = std::fs::read_dir(parent) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        if let Some(n) = path.file_name().and_then(|n| n.to_str()) {
            if n.starts_with(&prefix) {
                found.push(n.to_string());
            }
        }
    }
    found.sort();
    found
}

/// 见 [`pending_switch_datasets_at`]。
pub fn pending_switch_datasets(app: &tauri::AppHandle) -> Vec<String> {
    pending_switch_datasets_at(&base_data_dir(app))
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

#[cfg(test)]
#[path = "db.test.rs"]
mod tests;