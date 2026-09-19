//! 图像全屏查看窗口（独立窗口方案）：主窗口只发载荷，查看器跑在自己的窗口里。
//!
//! 为什么不用「主窗口切原生全屏」：Windows 退出全屏不是原子操作（先恢复窗口装饰、
//! 再套用最大化布局，WebView 客户区尺寸更新更晚），主窗口只要参与全屏状态，退出时
//! 必然出现「装饰已回来、客户区还是旧尺寸」的过渡帧，下层 UI 显示什么都躲不开
//! （两种表现：黑底+顶栏、或下层弹窗下沿跳动；详见 docs/lessons.md 第 18 节）。
//! 独立窗口方案下主窗口全程不动，退出即原地露出，没有任何窗口还原过程。
//!
//! 命令分工：
//! - `open_image_fullscreen`：主窗口调用（存载荷 + 通知 + 必要时隐藏创建窗口）；
//! - `mount_image_fullscreen`：查看器窗口挂载时取载荷（首次创建路径）；
//! - `show_image_fullscreen`：查看器窗口应用载荷并渲染完成后调用（显示 + 聚焦）；
//! - `close_image_fullscreen`：查看器窗口调用（隐藏窗口并聚焦主窗口；窗口复用不销毁）。

use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, EventTarget, Manager, State, WebviewUrl, WebviewWindowBuilder};
use tauri_specta::Event;

use crate::infra::error::AppError;

/// 查看器窗口 label：capabilities 按 label 授权，前端按 label 选择入口（见 src/main.ts）。
pub const WINDOW_LABEL: &str = "image-fullscreen";

/// 查看器列表项。`src` 允许空串：由查看器窗口按 id 惰性解析（`get_image_src` → `convertFileSrc`）。
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct ImageFullscreenItem {
    pub id: String,
    pub src: String,
    pub name: Option<String>,
    pub tags: Option<Vec<String>>,
}

/// 一次全屏查看的载荷：列表快照 + 起始索引。
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct ImageFullscreenPayload {
    pub items: Vec<ImageFullscreenItem>,
    pub index: usize,
}

/// 载荷中转：主窗口写入、查看器窗口读取。
/// 首次创建窗口时通知事件早于页面监听注册，所以「取」这条路径是必需的，不是冗余。
#[derive(Default)]
pub struct ImageFullscreenState(Mutex<Option<ImageFullscreenPayload>>);

/// 载荷就绪通知：窗口已存在时据它更新列表（首次创建由 `mount_image_fullscreen` 兜底）。
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, tauri_specta::Event)]
#[tauri_specta(event_name = "image-fullscreen-opened")]
pub struct ImageFullscreenOpened(pub ImageFullscreenPayload);

/// 窗口 API 错误统一转成可读的 AppError。
fn win_error(e: tauri::Error) -> AppError {
    AppError::Message(format!("窗口操作失败: {e}"))
}

/// 打开全屏查看（主窗口调用）。
#[tauri::command]
#[specta::specta]
pub async fn open_image_fullscreen(
    app: AppHandle,
    payload: ImageFullscreenPayload,
) -> Result<(), AppError> {
    {
        let state = app.state::<ImageFullscreenState>();
        let mut slot = state
            .0
            .lock()
            .map_err(|e| AppError::Message(e.to_string()))?;
        *slot = Some(payload.clone());
    }

    match app.get_webview_window(WINDOW_LABEL) {
        // 窗口已存在（隐藏中）：只推送新载荷；露面交给 show_image_fullscreen
        // （前端先应用载荷并渲染，再让窗口显示，避免复用窗口时闪一下上一张图）
        Some(_) => {
            ImageFullscreenOpened(payload)
                .emit_to(
                    &app,
                    EventTarget::AnyLabel {
                        label: WINDOW_LABEL.to_string(),
                    },
                )
                .map_err(win_error)?;
        }
        // 首次：隐藏创建，页面挂载后调 mount 取载荷
        None => create_window(&app)?,
    }
    Ok(())
}

/// 取本次载荷（查看器窗口挂载时调用；复用窗口的载荷走 `ImageFullscreenOpened` 事件）。
#[tauri::command]
#[specta::specta]
pub fn mount_image_fullscreen(
    state: State<'_, ImageFullscreenState>,
) -> Result<ImageFullscreenPayload, AppError> {
    state
        .0
        .lock()
        .map_err(|e| AppError::Message(e.to_string()))?
        .clone()
        .ok_or_else(|| AppError::Message("全屏查看缺少载荷".into()))
}

/// 显示并聚焦查看器窗口（查看器窗口应用载荷、渲染完成后自己调用）。
#[tauri::command]
#[specta::specta]
pub fn show_image_fullscreen(app: AppHandle) -> Result<(), AppError> {
    let win = app
        .get_webview_window(WINDOW_LABEL)
        .ok_or_else(|| AppError::Message("全屏查看窗口不存在".into()))?;
    win.show().map_err(win_error)?;
    win.set_focus().map_err(win_error)?;
    Ok(())
}

/// 关闭全屏查看：隐藏窗口（复用，不销毁）并聚焦主窗口 —— 主窗口原样露出。
#[tauri::command]
#[specta::specta]
pub fn close_image_fullscreen(app: AppHandle) -> Result<(), AppError> {
    if let Some(win) = app.get_webview_window(WINDOW_LABEL) {
        win.hide().map_err(win_error)?;
    }
    if let Some(main) = app.get_webview_window("main") {
        let _ = main.set_focus();
    }
    Ok(())
}

/// 创建查看器窗口：全屏 + 无装饰 + 隐藏启动（等页面挂载完成后由 `mount_image_fullscreen` 显示）。
fn create_window(app: &AppHandle) -> Result<(), AppError> {
    let win = WebviewWindowBuilder::new(app, WINDOW_LABEL, WebviewUrl::App("index.html".into()))
        .title("paim")
        .fullscreen(true)
        .decorations(false)
        .skip_taskbar(true)
        .visible(false)
        .build()
        .map_err(win_error)?;

    // Alt+F4 等同 ✕：只隐藏，避免窗口被销毁后与前端状态脱节（下次打开会重新创建）
    let handle = win.clone();
    win.on_window_event(move |event| {
        if let tauri::WindowEvent::CloseRequested { api, .. } = event {
            api.prevent_close();
            let _ = handle.hide();
            if let Some(main) = handle.app_handle().get_webview_window("main") {
                let _ = main.set_focus();
            }
        }
    });
    Ok(())
}
