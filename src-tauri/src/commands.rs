//! 命令层（order 2）：薄壳 Tauri 命令，只做参数校验与调用 domain，不写业务逻辑。
//! 新命令在此声明模块并注册到 lib.rs 的 `specta_builder()`。

pub mod backup;
pub mod image;
pub mod prompt;
pub mod stats;
pub mod tag;

use crate::infra::db::BkDb;
use crate::infra::error::AppError;
use rusqlite::Connection;
use tauri::State;

/// 查询型命令统一经此落到阻塞线程池：同步命令体在 Tauri 主线程执行，
/// 慢查询会冻结窗口消息循环（表现为「未响应」）；转 async + spawn_blocking 后
/// 查询再慢也只是慢，不再影响 UI。写命令未迁移（用户触发的单条写操作均毫秒级）。
pub async fn db_blocking<T, F>(db: &State<'_, BkDb>, f: F) -> Result<T, AppError>
where
    F: FnOnce(&Connection) -> Result<T, AppError> + Send + 'static,
    T: Send + 'static,
{
    let db = std::sync::Arc::clone(&db.0);
    tauri::async_runtime::spawn_blocking(move || {
        let conn = db.lock().map_err(|e| AppError::Message(e.to_string()))?;
        f(&conn)
    })
    .await
    .map_err(|e| AppError::Message(format!("查询任务执行失败: {e}")))?
}
