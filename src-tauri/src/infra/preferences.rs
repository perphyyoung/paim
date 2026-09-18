//! 用户偏好导出/导入。
//!
//! 「用户偏好」是前端 localStorage 里的界面设置（字号、字体家族、布局与排序等），
//! 它们与字体授权一起存在 WebView 目录（`EBWebView`）里——删该目录重新授权字体会连带丢掉，
//! 换机同理，故提供 JSON 文件的导出/导入。业务数据不在此列，那走「完整备份」。
//!
//! 本模块不自持状态：内容由前端序列化/解析，后端只做「文件读写 + 文件格式校验」
//! （校验 `app`/`kind`，避免把任意 JSON 当偏好导入）。也不引 `tauri-plugin-fs`：
//! 路径来自前端文件对话框，读写留在后端，零新依赖、零新增权限面。

use crate::infra::error::AppError;

/// 偏好文件的标识字段：只接受本应用导出的文件。
const FILE_APP: &str = "paim";
const FILE_KIND: &str = "preferences";

/// 校验偏好文件内容：必须是本应用导出的、带 `preferences` 对象的 JSON。
pub fn validate_preference_file(text: &str) -> Result<(), AppError> {
    let value: serde_json::Value = serde_json::from_str(text)
        .map_err(|e| AppError::Message(format!("不是合法的 JSON: {e}")))?;
    let app = value.get("app").and_then(|v| v.as_str());
    let kind = value.get("kind").and_then(|v| v.as_str());
    if app != Some(FILE_APP) || kind != Some(FILE_KIND) {
        return Err(AppError::Message("不是 paim 的偏好文件".into()));
    }
    if !value.get("preferences").is_some_and(|v| v.is_object()) {
        return Err(AppError::Message("偏好文件缺少 preferences 对象".into()));
    }
    Ok(())
}

/// 导出：把前端序列化好的偏好 JSON 写入指定路径。
#[tauri::command]
#[specta::specta]
pub fn export_preferences(path: String, json: String) -> Result<(), AppError> {
    validate_preference_file(&json)?;
    std::fs::write(&path, json).map_err(|e| AppError::Message(format!("写入偏好文件失败: {e}")))
}

/// 导入：读文件、校验后原样返回；写回 localStorage 由前端完成。
#[tauri::command]
#[specta::specta]
pub fn import_preferences(path: String) -> Result<String, AppError> {
    let text = std::fs::read_to_string(&path)
        .map_err(|e| AppError::Message(format!("读取偏好文件失败: {e}")))?;
    validate_preference_file(&text)?;
    Ok(text)
}

#[cfg(test)]
#[path = "preferences.test.rs"]
mod tests;
