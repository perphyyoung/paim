//! 命令层（order 2）：薄壳 Tauri 命令，只做参数校验与调用 domain，不写业务逻辑。
//! 新命令在此声明模块并注册到 lib.rs 的 `specta_builder()`。

pub mod backup;
pub mod e2e;
pub mod image;
pub mod image_fullscreen;
pub mod orphan;
pub mod prompt;
pub mod similarity;
pub mod stats;
pub mod tag;

use crate::infra::db::BkDb;
use crate::infra::error::AppError;
use rusqlite::Connection;
use std::future::Future;
use std::panic::Location;
use tauri::State;

/// 同步写命令的「慢」阈值：超过即 WARN。
pub const SLOW_WRITE_MS: u128 = 500;

/// 同步写命令的成对埋点守卫：`let _t = timed("update_prompt_detail");`
/// drop 时落「完成 + 耗时」（超过 [`SLOW_WRITE_MS`] 记 WARN），错误路径也会落。
/// 存在的理由：详情保存这类操作不依赖任何外部服务，卡住只可能发生在**取连接锁**或**写库**，
/// 而 `?` 提前返回时也要留下耗时，才能与 `db 锁等待/持有` 的 WARN 对上时间。
pub struct Timed {
    name: &'static str,
    started: std::time::Instant,
}

pub fn timed(name: &'static str) -> Timed {
    crate::log_debug!("{name} 开始");
    Timed {
        name,
        started: std::time::Instant::now(),
    }
}

impl Drop for Timed {
    fn drop(&mut self) {
        let ms = self.started.elapsed().as_millis();
        crate::log_debug!("{} 完成 耗时 {ms}ms", self.name);
        if ms >= SLOW_WRITE_MS {
            crate::log_warn!("{} 慢：{ms}ms", self.name);
        }
    }
}

/// 查询型命令统一经此落到阻塞线程池：同步命令体在 Tauri 主线程执行，
/// 慢查询会冻结窗口消息循环（表现为「未响应」）；转 async + spawn_blocking 后
/// 查询再慢也只是慢，不再影响 UI。
///
/// 形态是「非 async 函数返回 future」而不是 `async fn`：这样 `#[track_caller]` 才有意义
/// （它在 async fn 上是 no-op，见 rust-lang/rust#110011）。取到的调用点交给
/// [`crate::infra::db::DbConn::lock_at`]，锁等待 / 持锁告警就能点名到具体命令。
#[track_caller]
pub fn db_blocking<T, F>(
    db: &State<'_, BkDb>,
    f: F,
) -> impl Future<Output = Result<T, AppError>> + Send + 'static
where
    F: FnOnce(&Connection) -> Result<T, AppError> + Send + 'static,
    T: Send + 'static,
{
    let at = Location::caller();
    let db = std::sync::Arc::clone(&db.0);
    async move {
        tauri::async_runtime::spawn_blocking(move || {
            let conn = db.lock_at(at)?;
            f(&conn)
        })
        .await
        .map_err(|e| AppError::Message(format!("查询任务执行失败: {e}")))?
    }
}
