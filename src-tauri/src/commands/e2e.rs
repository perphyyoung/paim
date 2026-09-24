//! e2e 测试缝命令：供 e2e 测试操纵磁盘文件 / 读 DB 状态。
//! release 构建不门控（命令无害、无 UI 入口），统一简化维护。

use crate::infra::db::BkDb;
use crate::infra::error::AppError;
use rusqlite::OptionalExtension;
use serde::Serialize;
use specta::Type;
use tauri::{AppHandle, Manager, State};

/// e2e 测试缝：图像 DB 三列（thumbnail_path 空串表示 NULL）。
#[derive(Debug, Serialize, Type)]
pub struct E2EImageRecord {
    pub file_name: String,
    pub relative_path: String,
    pub thumbnail_path: String,
}

/// 删除指定图像的缩略图磁盘文件（不删 DB 记录、不删原图）。
/// 返回缩略图相对路径（用于 e2e 断言重建结果），若 DB 里没有 thumbnail_path 则返回 None。
#[tauri::command]
#[specta::specta]
pub fn e2e_delete_image_thumbnail(
    app: AppHandle,
    db: State<'_, BkDb>,
    image_id: String,
) -> Result<Option<String>, AppError> {
    let conn = db.0.lock().map_err(|e| AppError::Message(e.to_string()))?;
    let thumb_rel: Option<String> = conn
        .query_row(
            "SELECT thumbnail_path FROM images WHERE id = ?1",
            rusqlite::params![image_id],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| AppError::Message(e.to_string()))?;
    drop(conn);

    let Some(rel) = thumb_rel else {
        return Ok(None);
    };
    if rel.is_empty() {
        return Ok(None);
    }
    let full = crate::infra::db::app_data_path(&app, &rel);
    if full.exists() {
        std::fs::remove_file(&full)
            .map_err(|e| AppError::Message(format!("删除缩略图失败: {e}")))?;
    }
    Ok(Some(rel))
}

/// 读指定图像的 DB 记录：file_name / relative_path / thumbnail_path。
/// 不存在则返回 None；thumbnail_path 为 NULL 时空串。
#[tauri::command]
#[specta::specta]
pub fn e2e_get_image_paths(
    db: State<'_, BkDb>,
    image_id: String,
) -> Result<Option<E2EImageRecord>, AppError> {
    let conn = db.0.lock().map_err(|e| AppError::Message(e.to_string()))?;
    let row: Option<(String, String, String)> = conn
        .query_row(
            "SELECT file_name, relative_path, COALESCE(thumbnail_path, '') FROM images WHERE id = ?1",
            rusqlite::params![image_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()
        .map_err(|e| AppError::Message(e.to_string()))?;
    Ok(row.map(
        |(file_name, relative_path, thumbnail_path)| E2EImageRecord {
            file_name,
            relative_path,
            thumbnail_path,
        },
    ))
}

/// 读指定窗口当前是否可见（不存在则返回 None）。
/// 用途：查看器窗口是「隐藏复用」而非销毁（页面仍留在 CDP 里），而页面侧
/// `document.visibilityState` 不随窗口隐藏变化，只能从窗口侧判断「是否真的关掉了」。
#[tauri::command]
#[specta::specta]
pub fn e2e_is_window_visible(app: AppHandle, label: String) -> Result<Option<bool>, AppError> {
    let Some(win) = app.get_webview_window(&label) else {
        return Ok(None);
    };
    win.is_visible()
        .map(Some)
        .map_err(|e| AppError::Message(format!("读取窗口可见性失败: {e}")))
}
