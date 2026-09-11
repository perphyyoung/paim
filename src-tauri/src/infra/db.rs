//! 数据库连接管理与 schema 初始化。
//! 持久化细节集中在基础设施层，业务层通过 repository 接口访问。

use crate::infra::error::AppError;
use rusqlite::{Connection, OptionalExtension};
use std::path::{Path, PathBuf};
use tauri::State;

/// 应用持有的数据库连接（单连接 + Mutex），通过 Tauri managed state 注入。
/// Arc 包装使查询命令能克隆句柄丢进 `spawn_blocking`（见 commands::db_blocking）。
pub struct BkDb(pub std::sync::Arc<std::sync::Mutex<Connection>>);

/// 打开（必要时创建）数据库并执行 DDL。
/// 表名与字段名与 prompt-manager 完全一致，便于后续数据导入；
/// 时间列沿用本项目的 ISO 8601 UTC 约定（详见项目 memory）。
pub fn init(path: PathBuf) -> rusqlite::Result<BkDb> {
    Ok(BkDb(std::sync::Arc::new(std::sync::Mutex::new(
        open_connection(path)?,
    ))))
}

/// 打开（必要时创建）数据库并执行 DDL，返回裸连接。
/// 供 init 包装与备份导入「换连接」场景（backup_common::open_app_db）。
pub fn open_connection(path: PathBuf) -> rusqlite::Result<Connection> {
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
        -- 主页排序键（分页后 ORDER BY 走 SQL，万级数据不能 filesort）
        CREATE INDEX IF NOT EXISTS idx_images_active_file_size ON images(file_size) WHERE is_deleted = 0;
        -- 文件名排序已改 file_name（显示名口径），旧 stored_name 索引随迁移删除
DROP INDEX IF EXISTS idx_images_active_stored_name;
CREATE INDEX IF NOT EXISTS idx_images_active_file_name ON images(file_name) WHERE is_deleted = 0;
        CREATE INDEX IF NOT EXISTS idx_images_active_width ON images(width) WHERE is_deleted = 0;
        CREATE INDEX IF NOT EXISTS idx_images_active_height ON images(height) WHERE is_deleted = 0;
        CREATE INDEX IF NOT EXISTS idx_prompts_active_title ON prompts(title) WHERE is_deleted = 0;
        "#,
    )?;
    Ok(conn)
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
/// - 环境变量 PAIM_DATA_DIR 优先（e2e 测试用它指向 temp/ 下的隔离目录）；
/// - 开发环境（debug）使用项目根目录下的 paim-data（经编译期路径定位，
///   不依赖进程工作目录——tauri CLI 以 src-tauri 为 cwd 启动 exe）；
/// - 部署环境使用 Windows 默认的应用数据目录。
fn base_data_dir(app: &tauri::AppHandle) -> PathBuf {
    if let Ok(dir) = std::env::var("PAIM_DATA_DIR") {
        if !dir.is_empty() {
            return PathBuf::from(dir);
        }
    }
    if cfg!(debug_assertions) {
        project_root().join("paim-data")
    } else {
        default_data_dir(app)
    }
}

/// 项目根目录（src-tauri 的上级），经 CARGO_MANIFEST_DIR 编译期定位。
pub(crate) fn project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("manifest 必有父目录")
        .to_path_buf()
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

/// 临时目录：上传预览图（preview-<tag>）等运行中临时文件，下次正常启动时整体清空。
/// 必须位于数据目录之外：pm 备份导入会把整个数据目录改名让位，本目录若在其内，
/// 显示中的预览图句柄会让改名失败（os error 5）。
/// 注意：pm 备份导入的解压目录不在此处，用的是系统临时目录
/// （见 pm_backup_service.rs 的 create_temp_dir / std::env::temp_dir）。
/// - 开发环境：项目根下 temp/，与 paim-data 平级；
/// - 部署环境：应用缓存目录（LocalAppData/{identifier}/cache）下的 temp/。
pub fn temp_dir(app: &tauri::AppHandle) -> PathBuf {
    if cfg!(debug_assertions) {
        project_root().join("temp")
    } else {
        use tauri::Manager;
        app.path()
            .app_cache_dir()
            .expect("failed to resolve app cache dir")
            .join("temp")
    }
}

/// 清空临时目录（启动时调用），但跳过带实例标识的前缀：
/// - `e2e-*`/`wv2-*`：e2e 并行 worker 的数据目录与 WebView2 目录；
/// - `preview-*`：各实例的上传预览目录。
/// 这些目录由各自的实例/fixture 自行管理生命周期，清掉会影响别的实例。
pub fn clean_temp_dir(app: &tauri::AppHandle) {
    let root = temp_dir(app);
    let Ok(entries) = std::fs::read_dir(&root) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with("e2e-") || name.starts_with("wv2-") || name.starts_with("preview-") {
            continue;
        }
        let _ = std::fs::remove_dir_all(entry.path());
        let _ = std::fs::remove_file(entry.path());
    }
}

/// 实例标识：e2e 实例取 PAIM_DATA_DIR 的末段（如 e2e-w0），其余为 main。
/// 用于派生 per-instance 的临时子目录（如 preview-<tag>），并行实例互不干扰。
fn instance_tag() -> String {
    std::env::var("PAIM_DATA_DIR")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .and_then(|dir| {
            PathBuf::from(dir)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
        })
        .unwrap_or_else(|| "main".to_string())
}

/// 上传预览图目录：per-instance（temp/preview-<tag>），避免并行实例启动时
/// 清掉其他实例正在显示的预览图（webview 对显示中的图片持有句柄）。
pub fn preview_dir(app: &tauri::AppHandle) -> PathBuf {
    temp_dir(app).join(format!("preview-{}", instance_tag()))
}

/// 单元测试专用临时目录：<项目根>/temp/test/{name}-{nanos}/。
/// 与应用运行时 temp 同处一体，随应用下次正常启动一并清空，不落系统临时目录。
#[cfg(test)]
pub(crate) fn test_temp_dir(name: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let dir = project_root()
        .join("temp")
        .join("test")
        .join(format!("{name}-{nanos}"));
    std::fs::create_dir_all(&dir).expect("创建测试临时目录失败");
    dir
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
#[specta::specta]
pub fn get_data_dir(app: tauri::AppHandle) -> String {
    data_dir(&app).to_string_lossy().into_owned()
}

#[tauri::command]
#[specta::specta]
pub fn open_data_dir(app: tauri::AppHandle) -> Result<(), AppError> {
    let dir = data_dir(&app);
    std::process::Command::new("explorer")
        .arg(&dir)
        .spawn()
        .map_err(|e| AppError::Message(format!("打开目录失败: {e}")))?;
    Ok(())
}

/// 在资源管理器中定位并选中指定图像的本地保存文件（「打开本地保存位置」）。
/// 按图像 id 查库取得真实 relative_path（与前端拼接解耦，杜绝路径拼错）。
#[tauri::command]
#[specta::specta]
pub fn open_image_location(
    app: tauri::AppHandle,
    db: State<'_, BkDb>,
    id: String,
) -> Result<(), AppError> {
    let conn = db.0.lock().map_err(|e| AppError::Message(e.to_string()))?;
    let rel: Option<String> = conn
        .query_row(
            "SELECT relative_path FROM images WHERE id = ?1",
            rusqlite::params![id],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| AppError::Message(e.to_string()))?;
    drop(conn);
    let Some(rel) = rel.filter(|s| !s.is_empty()) else {
        return Err(AppError::Message("图像不存在或缺少保存路径".into()));
    };
    let full = data_dir(&app).join(rel);
    // 对齐 lap 的做法：explorer 参数拆分（/select, 与路径分开），
    // 且路径统一反斜杠（relative_path 含 /，混用分隔符会让 explorer 回退默认位置）
    let norm = full.to_string_lossy().replace('/', "\\");
    std::process::Command::new("explorer")
        .arg("/select,")
        .arg(norm)
        .spawn()
        .map_err(|e| AppError::Message(format!("打开保存位置失败: {e}")))?;
    Ok(())
}

/// 批量切换收藏（对齐 pm：集合级 `1 - is_favorite` 一次 SQL，收藏↔取消收藏）
fn toggle_favorite(db: &State<'_, BkDb>, table: &str, ids: Vec<String>) -> Result<usize, AppError> {
    if ids.is_empty() {
        return Ok(0);
    }
    let conn = db.0.lock().map_err(|e| AppError::Message(e.to_string()))?;
    let placeholders = vec!["?"; ids.len()].join(",");
    let n = conn
        .execute(
            &format!(
                "UPDATE {table} SET is_favorite = 1 - is_favorite WHERE id IN ({placeholders})"
            ),
            rusqlite::params_from_iter(ids.iter()),
        )
        .map_err(|e| AppError::Message(e.to_string()))?;
    Ok(n)
}

#[tauri::command]
#[specta::specta]
pub fn batch_toggle_image_favorite(
    db: State<'_, BkDb>,
    ids: Vec<String>,
) -> Result<usize, AppError> {
    toggle_favorite(&db, "images", ids)
}

#[tauri::command]
#[specta::specta]
pub fn batch_toggle_prompt_favorite(
    db: State<'_, BkDb>,
    ids: Vec<String>,
) -> Result<usize, AppError> {
    toggle_favorite(&db, "prompts", ids)
}

#[cfg(test)]
#[path = "db.test.rs"]
mod tests;
