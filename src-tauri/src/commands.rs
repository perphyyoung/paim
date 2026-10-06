//! 命令层（order 2）：薄壳 Tauri 命令，只做参数校验与调用 domain，不写业务逻辑。
//! 新命令在此声明模块并注册到 lib.rs 的 `specta_builder()`。
//!
//! 命令形态唯一约定：**一律 `async fn` + `crate::infra::db::blocking`**（访问数据库时），
//! 或 `spawn_blocking`（纯文件 IO / 编解码 / 系统调用）。理由见 [`crate::infra::db::blocking`]
//! 的文档：同步命令占宿主主线程会冻结窗口，`#[tauri::command(async)]` 又会占住 async 工作线程，
//! 两者都不行——阻塞工作只能落在专用阻塞池。

pub mod backup;
pub mod e2e;
pub mod image;
pub mod image_fullscreen;
pub mod orphan;
pub mod prompt;
pub mod similarity;
pub mod stats;
pub mod tag;

/// 写命令的「慢」阈值：超过即 WARN。
pub const SLOW_WRITE_MS: u128 = 500;

/// 写命令的成对埋点守卫：`let _t = timed("update_prompt_detail");`
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
