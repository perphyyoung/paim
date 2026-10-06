//! 数据完整性检查命令。
//!
//! 两条命令：
//! - `scan_integrity`：完整检查（孤儿文件 + 孤儿记录），返回 IntegrityCheckResult
//! - `export_orphan_files`：导出并删除孤儿文件（原图像复制到用户选目录再删、缩略图直接删）

use crate::domain::orphan_file_service::{self, IntegrityCheckResult, OrphanExportResult};
use crate::infra::db::{self, BkDb};
use crate::infra::error::AppError;
use crate::infra::time;

/// 数据完整性检查（扫描磁盘，重 IO）：走阻塞池，不占主线程，也不占 async 工作线程。
#[tauri::command]
#[specta::specta]
pub async fn scan_integrity(
    app: tauri::AppHandle,
    db: tauri::State<'_, BkDb>,
) -> Result<IntegrityCheckResult, AppError> {
    let data_dir = db::data_dir(&app);
    db::blocking(&db, move |conn| {
        orphan_file_service::scan_integrity(conn, &data_dir)
            .map_err(|e| AppError::Message(e.to_string()))
    })
    .await
}

/// 导出并删除孤儿文件。
/// `export_dir` 由前端通过目录选择器拿到，命令内部建 `orphan_files_<时间戳>/` 子目录；
/// 时间戳格式见 [`crate::infra::time::file_stamp`]（与备份导出、让位备份目录同一格式）。
#[tauri::command]
#[specta::specta]
pub async fn export_orphan_files(
    app: tauri::AppHandle,
    db: tauri::State<'_, BkDb>,
    export_dir: String,
) -> Result<OrphanExportResult, AppError> {
    let data_dir = db::data_dir(&app);
    let orphan_export_dir =
        std::path::PathBuf::from(&export_dir).join(format!("orphan_files_{}", time::file_stamp()));
    // 建目录 + 复制 + 删除孤儿文件都是重 IO，放阻塞池（与完整性检查同口径）
    db::blocking(&db, move |conn| {
        std::fs::create_dir_all(&orphan_export_dir)
            .map_err(|e| AppError::Message(format!("创建导出目录失败: {e}")))?;
        orphan_file_service::export_orphan_files(conn, &data_dir, &orphan_export_dir)
            .map_err(|e| AppError::Message(e.to_string()))
    })
    .await
}
