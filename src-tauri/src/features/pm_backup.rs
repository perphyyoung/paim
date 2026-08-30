//! pm(prompt-manager)备份导入命令层：薄适配，从 managed state 取连接，转调导入服务，
//! 并把服务进度回调桥接为前端事件推送。

use crate::db::BkDb;
use crate::features::pm_backup_service::{self, ImportProgress, PmBackupInfo, PmImportSummary};
use tauri::{Emitter, State};

/// 解析 pm 备份包，返回内容概览（不改动本地数据）。
#[tauri::command]
pub fn inspect_pm_backup(zip_path: String) -> Result<PmBackupInfo, String> {
    pm_backup_service::inspect(&zip_path)
}

/// 导入 pm 全量备份（整体替换当前数据），进度经 pm-import-progress 事件推送。
#[tauri::command]
pub fn import_pm_backup(
    app: tauri::AppHandle,
    db: State<BkDb>,
    zip_path: String,
) -> Result<PmImportSummary, String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    pm_backup_service::import(&app, &conn, &zip_path, |p: ImportProgress| {
        let _ = app.emit(pm_backup_service::PROGRESS_EVENT, p);
    })
}
