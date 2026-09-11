//! 极简调试日志：追加写入「当前工作目录 / paim.log」。
//! 服务两类调用方：
//! - 后端关键路径埋点（log_info!/log_warn!/log_error! 宏）
//! - 前端通过 tauri 命令 `log_msg` 上报（invoke）
//! 前后端共用一个全局最低级别开关（MIN_LEVEL）：启动时读环境变量 PAIM_LOG，
//! 运行时经 set_log_level 命令热切并 emit 事件同步前端缓存。
//! 写文件失败时静默，不阻塞业务（调试用途）。

use serde::{Deserialize, Serialize};
use std::fs::OpenOptions;
use std::io::Write;
use std::sync::OnceLock;
use tauri_specta::Event;

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

/// 全局最低输出级别（0=Debug 1=Info 2=Warn 3=Error），默认 Info。
static MIN_LEVEL: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(1);

fn level_num(level: Level) -> u8 {
    match level {
        Level::Debug => 0,
        Level::Info => 1,
        Level::Warn => 2,
        Level::Error => 3,
    }
}

fn level_from_str(s: &str) -> Option<Level> {
    match s {
        "debug" => Some(Level::Debug),
        "info" => Some(Level::Info),
        "warn" => Some(Level::Warn),
        "error" => Some(Level::Error),
        _ => None,
    }
}

/// 启动初始化：读环境变量 `PAIM_LOG`（debug/info/warn/error，缺省 info）。
pub fn init_from_env() {
    if let Some(level) = std::env::var("PAIM_LOG")
        .ok()
        .and_then(|s| level_from_str(&s.to_ascii_lowercase()))
    {
        set_min_level(level);
    }
}

pub fn set_min_level(level: Level) {
    MIN_LEVEL.store(level_num(level), std::sync::atomic::Ordering::Relaxed);
}

fn min_level() -> Level {
    match MIN_LEVEL.load(std::sync::atomic::Ordering::Relaxed) {
        0 => Level::Debug,
        2 => Level::Warn,
        3 => Level::Error,
        _ => Level::Info,
    }
}

/// 级别是否达到全局最低线（宏在 format! 前调用，被过滤的日志零格式化开销）。
pub fn enabled(level: Level) -> bool {
    level_num(level) >= MIN_LEVEL.load(std::sync::atomic::Ordering::Relaxed)
}

/// 日志级别变更事件（payload 为新级别小写字符串），前端监听后刷新本地缓存。
#[derive(Debug, Serialize, Deserialize, Clone, specta::Type, tauri_specta::Event)]
#[tauri_specta(event_name = "log-level-changed")]
pub struct LogLevelChanged(pub String);

/// 日志文件路径，惰性计算一次：
/// - 开发环境（debug）：项目根 / paim.log（e2e/dev 都写这里，便于排查）；
/// - 部署环境：进程工作目录 / paim.log。
fn log_path() -> &'static std::path::PathBuf {
    static PATH: OnceLock<std::path::PathBuf> = OnceLock::new();
    PATH.get_or_init(|| {
        if cfg!(debug_assertions) {
            crate::infra::db::project_root().join("paim.log")
        } else {
            std::env::current_dir().unwrap_or_default().join("paim.log")
        }
    })
}

/// 写一行日志：`本地时间 [级别] 消息`。
pub fn write(level: Level, msg: impl AsRef<str>) {
    let line = format!("{} [{}] {}\n", now_local(), level.as_str(), msg.as_ref());
    if let Ok(mut f) = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path())
    {
        let _ = f.write_all(line.as_bytes());
    }
}

/// 日志宏（导出供 crate 内模块调用）：先判级别再格式化，被过滤的日志零开销。
#[macro_export]
macro_rules! log_debug {
    ($($arg:tt)*) => { if $crate::infra::logging::enabled($crate::infra::logging::Level::Debug) { $crate::infra::logging::write($crate::infra::logging::Level::Debug, format!($($arg)*)) } };
}
#[macro_export]
macro_rules! log_info {
    ($($arg:tt)*) => { if $crate::infra::logging::enabled($crate::infra::logging::Level::Info) { $crate::infra::logging::write($crate::infra::logging::Level::Info, format!($($arg)*)) } };
}
#[macro_export]
macro_rules! log_warn {
    ($($arg:tt)*) => { if $crate::infra::logging::enabled($crate::infra::logging::Level::Warn) { $crate::infra::logging::write($crate::infra::logging::Level::Warn, format!($($arg)*)) } };
}
#[macro_export]
macro_rules! log_error {
    ($($arg:tt)*) => { if $crate::infra::logging::enabled($crate::infra::logging::Level::Error) { $crate::infra::logging::write($crate::infra::logging::Level::Error, format!($($arg)*)) } };
}

/// 前端上报日志：`rtk invoke("log_msg", { level, message })`。
/// 经全局级别过滤，被过滤的日志不落盘（前端有本地缓存预判，正常不会走到这）。
#[tauri::command]
#[specta::specta]
pub fn log_msg(level: String, message: String) {
    let lvl = match level.as_str() {
        "debug" => Level::Debug,
        "warn" => Level::Warn,
        "error" => Level::Error,
        _ => Level::Info,
    };
    if !enabled(lvl) {
        return;
    }
    write(lvl, format!("[FE] {}", message));
}

/// 查询当前全局最低日志级别（小写字符串，供前端启动时同步缓存）。
#[tauri::command]
#[specta::specta]
pub fn get_log_level() -> String {
    match min_level() {
        Level::Debug => "debug",
        Level::Info => "info",
        Level::Warn => "warn",
        Level::Error => "error",
    }
    .to_string()
}

/// 运行时热切全局最低日志级别，并 emit 事件让前端刷新缓存。
#[tauri::command]
#[specta::specta]
pub fn set_log_level(app: tauri::AppHandle, level: String) -> Result<(), String> {
    let lowered = level.to_ascii_lowercase();
    let lvl = level_from_str(&lowered)
        .ok_or_else(|| format!("无效日志级别: {level}（可选 debug/info/warn/error）"))?;
    set_min_level(lvl);
    write(
        Level::Info,
        format!("[log] 最低级别切换为 {}", lvl.as_str()),
    );
    let _ = LogLevelChanged(lowered).emit(&app);
    Ok(())
}

/// 当前本地时间（毫秒精度，形如 2026-08-23 12:00:00.123）。
fn now_local() -> String {
    chrono::Local::now()
        .format("%Y-%m-%d %H:%M:%S%.3f")
        .to_string()
}
