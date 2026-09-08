//! 统计命令层：薄适配，从 managed state 取连接，转调 statistics_service。

use crate::domain::statistics_service::{self, Statistics};
use crate::infra::db::BkDb;
use crate::infra::error::AppError;

use tauri::State;

/// 返回全局数据统计（12 项，与 pm 统计弹窗对齐），每次调用实时查询。
#[tauri::command]
#[specta::specta]
pub fn get_statistics(db: State<BkDb>) -> Result<Statistics, AppError> {
    let conn = db.0.lock().map_err(|e| AppError::Message(e.to_string()))?;
    statistics_service::get(&conn).map_err(|e| AppError::Message(e.to_string()))
}
