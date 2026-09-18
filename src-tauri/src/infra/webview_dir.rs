//! WebView 目录（WebView2 的 `EBWebView`）：字体授权决定、localStorage（界面偏好）、缓存都在这里。
//!
//! 路径：`%LOCALAPPDATA%\<identifier>\EBWebView`。Tauri 在 Windows 上强制把 WebView2 的
//! user data folder 设为 `%LOCALAPPDATA%\<identifier>`（`BaseDirectory::LocalData` + 应用标识符，
//! 见 tauri 的 `manager/webview.rs`；该分支不看 debug/release，故开发版与安装版同路径），
//! `EBWebView` 是 WebView2 在其下自建的子目录。
//!
//! 为什么需要它：字体访问授权被拒后浏览器不再弹授权框，而 JS 侧没有撤销权限的 API
//! （`permissions.revoke` 未实现），只能退出应用后删掉 `EBWebView` 再重启。
//!
//! 术语：本模块一律称「WebView 目录」（即 `EBWebView`），**不要与设置页的「数据目录」混用**——
//! 数据目录放业务数据（`paim.db` + `images`），删 WebView 目录不影响它。

use crate::infra::error::AppError;
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

/// WebView2 的 user data folder（`%LOCALAPPDATA%\<identifier>`），`EBWebView` 在其下。
fn user_data_dir(app: &AppHandle) -> Result<PathBuf, AppError> {
    app.path()
        .app_local_data_dir()
        .map_err(|e| AppError::Message(format!("无法取得 WebView 目录: {e}")))
}

/// WebView 目录（重新授权时要删除的那个）。
pub fn webview_dir(app: &AppHandle) -> Result<PathBuf, AppError> {
    Ok(user_data_dir(app)?.join("EBWebView"))
}

/// 供界面显示「重新授权」指引里的具体路径（精确到 `EBWebView`）。
#[tauri::command]
#[specta::specta]
pub fn get_webview_dir(app: AppHandle) -> Result<String, AppError> {
    Ok(webview_dir(&app)?.to_string_lossy().into_owned())
}

/// 在资源管理器中打开 WebView 目录：`EBWebView` 存在则打开其父目录并**选中它**
/// （站在要删的目录里是删不掉自己的，选中后可以直接删）；不存在则退回打开父目录。
#[tauri::command]
#[specta::specta]
pub fn open_webview_dir(app: AppHandle) -> Result<(), AppError> {
    let udf = user_data_dir(&app)?;
    std::fs::create_dir_all(&udf)
        .map_err(|e| AppError::Message(format!("创建 WebView 目录失败: {e}")))?;
    let target = webview_dir(&app)?;
    if target.exists() {
        crate::infra::shell_explorer::reveal_in_explorer(&target)
    } else {
        crate::infra::shell_explorer::open_in_explorer(&udf)
    }
}
