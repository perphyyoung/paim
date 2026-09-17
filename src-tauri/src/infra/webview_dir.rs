//! WebView 用户数据目录（WebView2 的 user data folder）。
//!
//! Tauri 在 Windows 上强制把 WebView2 的 data directory 设为
//! `%LOCALAPPDATA%\<identifier>`（`BaseDirectory::LocalData` + 应用标识符，见 tauri 的
//! `manager/webview.rs`，该分支不看 debug/release，故开发版与安装版同路径），
//! WebView2 再在其下创建 `EBWebView`。
//!
//! 为什么需要它：**字体访问授权决定、localStorage（界面偏好）、缓存都在 `EBWebView` 里**。
//! 权限被拒后浏览器不再弹授权框，而 JS 侧没有撤销权限的 API（`permissions.revoke` 未实现），
//! 只能删除 `EBWebView` 后重启应用重新授权（必须先退出应用，否则文件被占用删不掉）。
//! 与业务数据无关——业务数据在设置页「数据目录」里。
//!
//! 注意术语：本模块一律称「WebView 目录」，不要与「数据目录」混用。

use crate::infra::error::AppError;
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

/// WebView 目录；`EBWebView` 是其子目录。
pub fn webview_dir(app: &AppHandle) -> Result<PathBuf, AppError> {
    app.path()
        .app_local_data_dir()
        .map_err(|e| AppError::Message(format!("无法取得 WebView 目录: {e}")))
}

/// 供界面显示「重新授权」指引里的具体路径。
#[tauri::command]
#[specta::specta]
pub fn get_webview_dir(app: AppHandle) -> Result<String, AppError> {
    Ok(webview_dir(&app)?.to_string_lossy().into_owned())
}

/// 在资源管理器中打开 WebView 目录（指引里的一键打开，省得用户手拼路径）。
#[tauri::command]
#[specta::specta]
pub fn open_webview_dir(app: AppHandle) -> Result<(), AppError> {
    let dir = webview_dir(&app)?;
    // 兜底：极端情况下（如用户把整个目录删了）先建出来再打开，否则 Explorer 定位失败
    std::fs::create_dir_all(&dir)
        .map_err(|e| AppError::Message(format!("创建 WebView 目录失败: {e}")))?;
    crate::infra::shell_explorer::open_in_explorer(&dir)
}
