//! 孤儿文件清理命令（薄壳：调 domain::orphan_file_service）。
//!
//! 两条命令：
//! - `scan_orphan_files`：只扫不删，返回数量
//! - `export_orphan_files`：一步导出并删除（原图像复制到用户选目录再删、缩略图直接删）

use crate::domain::orphan_file_service::{self, OrphanExportResult, OrphanScanResult};
use crate::infra::db::BkDb;
use crate::infra::error::AppError;
use std::time::{SystemTime, UNIX_EPOCH};

/// 扫描孤儿文件（阻塞，在 spawn_blocking 内执行）
#[tauri::command]
#[specta::specta]
pub async fn scan_orphan_files(
    app: tauri::AppHandle,
    db: tauri::State<'_, BkDb>,
) -> Result<OrphanScanResult, AppError> {
    let data_dir = crate::infra::db::data_dir(&app);
    let conn = db.0.lock().map_err(|e| AppError::Message(e.to_string()))?;
    orphan_file_service::scan_orphan_files(&conn, &data_dir)
        .map_err(|e| AppError::Message(e.to_string()))
}

/// 导出并删除孤儿文件。
/// `export_dir` 由前端通过目录选择器拿到，命令内部建 `orphan_files_{时间戳}/` 子目录。
#[tauri::command]
#[specta::specta]
pub async fn export_orphan_files(
    app: tauri::AppHandle,
    db: tauri::State<'_, BkDb>,
    export_dir: String,
) -> Result<OrphanExportResult, AppError> {
    let data_dir = crate::infra::db::data_dir(&app);

    let base = std::path::PathBuf::from(&export_dir);
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let orphan_export_dir = base.join(format!("orphan_files_{stamp}"));
    std::fs::create_dir_all(&orphan_export_dir)
        .map_err(|e| AppError::Message(format!("创建导出目录失败: {e}")))?;

    let conn = db.0.lock().map_err(|e| AppError::Message(e.to_string()))?;
    orphan_file_service::export_orphan_files(&conn, &data_dir, &orphan_export_dir)
        .map_err(|e| AppError::Message(e.to_string()))
}
