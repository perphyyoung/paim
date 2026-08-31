//! pm(prompt-manager)备份导入命令层：薄适配，转调导入服务，
//! 并把服务进度回调桥接为前端事件推送。
//! 两个命令都是重 IO 长任务：以 async + spawn_blocking 执行，
//! 避免同步命令在主线程运行导致窗口“未响应”。

use crate::db::BkDb;
use crate::error::AppError;
use crate::features::pm_backup_service::{self, ImportProgress, PmBackupInfo, PmImportSummary};
use tauri::{Emitter, Manager};

/// 解析 pm 备份包，返回内容概览（不改动本地数据）。
#[tauri::command]
pub async fn inspect_pm_backup(zip_path: String) -> Result<PmBackupInfo, AppError> {
    tauri::async_runtime::spawn_blocking(move || pm_backup_service::inspect(&zip_path))
        .await
        .map_err(|e| AppError::Message(format!("备份解析任务失败: {e}")))?
        .map_err(AppError::from)
}

/// 导入 pm 全量备份（整体替换当前数据），进度经 pm-import-progress 事件推送。
#[tauri::command]
pub async fn import_pm_backup(
    app: tauri::AppHandle,
    zip_path: String,
) -> Result<PmImportSummary, AppError> {
    tauri::async_runtime::spawn_blocking(move || {
        let bk = app.state::<BkDb>();
        pm_backup_service::import(&app, &bk, &zip_path, |p: ImportProgress| {
            let _ = app.emit(pm_backup_service::PROGRESS_EVENT, p);
        })
    })
    .await
    .map_err(|e| AppError::Message(format!("导入任务执行失败: {e}")))?
    .map_err(AppError::from)
}
