//! paim 自有备份导出/导入命令层：薄适配，转调备份服务，
//! 并把服务进度回调桥接为前端事件推送。
//! 三个命令都是重 IO 长任务：以 async + spawn_blocking 执行，
//! 避免同步命令在主线程运行导致窗口“未响应”。

use crate::domain::paim_backup_service::{
    self, PaimBackupInfo, PaimBackupProgress, PaimExportSummary, PaimImportSummary,
};
use crate::infra::db::BkDb;
use crate::infra::error::AppError;
use tauri::Manager;
use tauri_specta::Event;

/// 解析 paim 备份包，返回内容概览（不改动本地数据）。
#[tauri::command]
#[specta::specta]
pub async fn inspect_paim_backup(zip_path: String) -> Result<PaimBackupInfo, AppError> {
    tauri::async_runtime::spawn_blocking(move || paim_backup_service::inspect(&zip_path))
        .await
        .map_err(|e| AppError::Message(format!("备份解析任务失败: {e}")))?
        .map_err(AppError::from)
}

/// 导出 paim 全量备份到指定 ZIP 路径，进度经 paim-backup-progress 事件推送。
#[tauri::command]
#[specta::specta]
pub async fn export_paim_backup(
    app: tauri::AppHandle,
    export_path: String,
) -> Result<PaimExportSummary, AppError> {
    tauri::async_runtime::spawn_blocking(move || {
        let bk = app.state::<BkDb>();
        paim_backup_service::export(&app, &bk, &export_path, |p: PaimBackupProgress| {
            let _ = p.emit(&app);
        })
    })
    .await
    .map_err(|e| AppError::Message(format!("导出任务执行失败: {e}")))?
    .map_err(AppError::from)
}

/// 导入 paim 全量备份（整体替换当前数据），进度经 paim-backup-progress 事件推送。
#[tauri::command]
#[specta::specta]
pub async fn import_paim_backup(
    app: tauri::AppHandle,
    zip_path: String,
) -> Result<PaimImportSummary, AppError> {
    tauri::async_runtime::spawn_blocking(move || {
        let bk = app.state::<BkDb>();
        paim_backup_service::import(&app, &bk, &zip_path, |p: PaimBackupProgress| {
            let _ = p.emit(&app);
        })
    })
    .await
    .map_err(|e| AppError::Message(format!("导入任务执行失败: {e}")))?
    .map_err(AppError::from)
}
