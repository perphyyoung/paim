//! 完整备份命令层：导入经 manifest 探测自动分发 paim/pm，导出仅 paim 自有数据。
//! 薄适配 + 进度事件桥接；均为重 IO 长任务，以 async + spawn_blocking 执行，
//! 避免同步命令在主线程运行导致窗口“未响应”。

use crate::domain::backup_common::{
    detect_app, BackupExportSummary, BackupImportSummary, BackupInfo, BackupProgress,
};
use crate::domain::{paim_backup_service, pm_backup_service};
use crate::infra::db::BkDb;
use crate::infra::error::AppError;
use tauri::Manager;
use tauri_specta::Event;

/// 解析备份包（自动识别 paim/pm），返回内容概览（不改动本地数据）。
#[tauri::command]
#[specta::specta]
pub async fn inspect_backup(zip_path: String) -> Result<BackupInfo, AppError> {
    tauri::async_runtime::spawn_blocking(move || match detect_app(&zip_path)?.as_str() {
        "pm" => pm_backup_service::inspect(&zip_path),
        _ => paim_backup_service::inspect(&zip_path),
    })
    .await
    .map_err(|e| AppError::Message(format!("备份解析任务失败: {e}")))?
    .map_err(AppError::from)
}

/// 导出 paim 全量备份到指定 ZIP 路径，进度经 backup-progress 事件推送。
#[tauri::command]
#[specta::specta]
pub async fn export_backup(
    app: tauri::AppHandle,
    export_path: String,
) -> Result<BackupExportSummary, AppError> {
    tauri::async_runtime::spawn_blocking(move || {
        let bk = app.state::<BkDb>();
        paim_backup_service::export(&app, &bk, &export_path, |p: BackupProgress| {
            let _ = p.emit(&app);
        })
    })
    .await
    .map_err(|e| AppError::Message(format!("导出任务执行失败: {e}")))?
    .map_err(AppError::from)
}

/// 导入全量备份（自动识别 paim/pm；整体替换当前数据），进度经 backup-progress 事件推送。
#[tauri::command]
#[specta::specta]
pub async fn import_backup(
    app: tauri::AppHandle,
    zip_path: String,
) -> Result<BackupImportSummary, AppError> {
    tauri::async_runtime::spawn_blocking(move || {
        let bk = app.state::<BkDb>();
        let emit = |p: BackupProgress| {
            let _ = p.emit(&app);
        };
        match detect_app(&zip_path)?.as_str() {
            "pm" => pm_backup_service::import(&app, &bk, &zip_path, emit),
            _ => paim_backup_service::import(&app, &bk, &zip_path, emit),
        }
    })
    .await
    .map_err(|e| AppError::Message(format!("导入任务执行失败: {e}")))?
    .map_err(AppError::from)
}
