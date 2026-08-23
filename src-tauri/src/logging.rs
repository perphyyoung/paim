//! 极简调试日志：追加写入「当前工作目录 / paim.log」。
//! 服务两类调用方：
//! - 后端关键路径埋点（log_info!/log_warn!/log_error! 宏）
//! - 前端通过 tauri 命令 `log_msg` 上报（invoke）
//! 写文件失败时静默，不阻塞业务（调试用途）。

use std::fs::OpenOptions;
use std::io::Write;
use std::sync::OnceLock;

/// 日志级别。
#[derive(Clone, Copy, PartialEq)]
pub enum Level {
    Debug,
    Info,
    Warn,
    Error,
}

impl Level {
    fn as_str(&self) -> &'static str {
        match self {
            Level::Debug => "DEBUG",
            Level::Info => "INFO",
            Level::Warn => "WARN",
            Level::Error => "ERROR",
        }
    }
}

/// 日志文件路径：当前工作目录 / paim.log，惰性计算一次。
fn log_path() -> &'static std::path::PathBuf {
    static PATH: OnceLock<std::path::PathBuf> = OnceLock::new();
    PATH.get_or_init(|| std::env::current_dir().unwrap_or_default().join("paim.log"))
}

/// 写一行日志：`本地时间 [级别] 消息`。
pub fn write(level: Level, msg: impl AsRef<str>) {
    let line = format!("{} [{}] {}\n", now_local(), level.as_str(), msg.as_ref());
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(log_path()) {
        let _ = f.write_all(line.as_bytes());
    }
}

/// 日志宏（导出供 crate 内模块调用）。
#[macro_export]
macro_rules! log_debug {
    ($($arg:tt)*) => { $crate::logging::write($crate::logging::Level::Debug, format!($($arg)*)) };
}
#[macro_export]
macro_rules! log_info {
    ($($arg:tt)*) => { $crate::logging::write($crate::logging::Level::Info, format!($($arg)*)) };
}
#[macro_export]
macro_rules! log_warn {
    ($($arg:tt)*) => { $crate::logging::write($crate::logging::Level::Warn, format!($($arg)*)) };
}
#[macro_export]
macro_rules! log_error {
    ($($arg:tt)*) => { $crate::logging::write($crate::logging::Level::Error, format!($($arg)*)) };
}

/// 前端上报日志：`rtk invoke("log_msg", { level, message })`。
#[tauri::command]
pub fn log_msg(level: String, message: String) {
    let lvl = match level.as_str() {
        "debug" => Level::Debug,
        "warn" => Level::Warn,
        "error" => Level::Error,
        _ => Level::Info,
    };
    write(lvl, format!("[FE] {}", message));
}

/// 当前本地时间（毫秒精度，形如 2026-08-23 12:00:00.123）。
fn now_local() -> String {
    chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f").to_string()
}