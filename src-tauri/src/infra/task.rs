//! 阻塞任务的统一投放口：把文件 IO / 编解码 / 系统调用等阻塞工作丢进**专用阻塞线程池**。
//!
//! 与 [`crate::infra::db::blocking`] 的分工：涉及数据库（等连接锁）用它，纯阻塞工作用这里。
//!
//! 为什么必须有这一层（与 db::blocking 同一套理由，见 docs/lessons.md 第 12 / 28 节）：
//! - 不带 `async` 的 Tauri 命令在宿主**主线程内联执行**，阻塞工作会冻结窗口消息循环；
//! - `#[tauri::command(async)]` 也不是答案——宏给同步函数体生成的代码只是把它放进
//!   `async_runtime::spawn` 的 async 任务里，阻塞会占住 **async 工作线程**（默认按核数）；
//! - 只有 `spawn_blocking` 用的独立阻塞池能承载「几十毫秒到几分钟」的阻塞工作。

use crate::infra::error::AppError;
use std::future::Future;

/// 在阻塞线程池里执行 `f`；`what` 用于拼装 JoinError 的可读前缀（如 "缩略图自愈"）。
pub fn spawn_blocking<T, F>(
    what: &'static str,
    f: F,
) -> impl Future<Output = Result<T, AppError>> + Send + 'static
where
    F: FnOnce() -> Result<T, AppError> + Send + 'static,
    T: Send + 'static,
{
    async move {
        tauri::async_runtime::spawn_blocking(f)
            .await
            .map_err(|e| AppError::Message(format!("{what}任务执行失败: {e}")))?
    }
}
