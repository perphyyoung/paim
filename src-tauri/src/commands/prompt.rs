//! 提示词命令层：薄适配，从 managed state 取连接，转调领域服务。
//! 路径 `commands::prompt::`，与图像侧 `commands::image::` 对称。

use crate::domain::image_service;
use crate::domain::prompt_service;
use crate::infra::db::BkDb;
use crate::infra::error::AppError;

use serde::Serialize;
use tauri::State;

#[tauri::command]
#[specta::specta]
pub fn list_prompts(db: State<BkDb>) -> Result<Vec<prompt_service::Prompt>, AppError> {
    let conn = db.0.lock().map_err(|e| AppError::Message(e.to_string()))?;
    prompt_service::list(&conn).map_err(|e| AppError::Message(e.to_string()))
}

/// 返回每个提示词关联（未删除）的图像数：{promptId: count}，供「有图」特殊标签与排序。
#[tauri::command]
#[specta::specta]
pub fn get_prompt_images_count_map(
    db: State<BkDb>,
) -> Result<std::collections::HashMap<String, i64>, AppError> {
    let conn = db.0.lock().map_err(|e| AppError::Message(e.to_string()))?;
    let mut stmt = conn
        .prepare(
            "SELECT pir.prompt_id, COUNT(*)
             FROM prompt_image_relations pir
             JOIN images img ON img.id = pir.image_id
             WHERE img.is_deleted = 0
             GROUP BY pir.prompt_id",
        )
        .map_err(|e| AppError::Message(e.to_string()))?;
    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))
        .map_err(|e| AppError::Message(e.to_string()))?;
    let mut map: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
    for row in rows {
        let (pid, count) = row.map_err(|e| AppError::Message(e.to_string()))?;
        map.insert(pid, count);
    }
    Ok(map)
}

#[derive(Debug, Serialize, Clone, specta::Type)]
pub struct CreatePromptWithImagesResult {
    pub prompt: prompt_service::Prompt,
    pub results: Vec<crate::domain::image_service::ImageImportResult>,
    pub errors: Vec<crate::domain::image_service::ImageImportError>,
}

/// 新建提示词（内容必需）；image_paths 非空时上传并关联到该提示词。
#[tauri::command]
#[specta::specta]
pub fn create_prompt_with_images(
    db: State<BkDb>,
    app: tauri::AppHandle,
    content: String,
    title: Option<String>,
    image_paths: Vec<String>,
) -> Result<CreatePromptWithImagesResult, AppError> {
    let conn = db.0.lock().map_err(|e| AppError::Message(e.to_string()))?;
    let prompt = prompt_service::create(&conn, &content, title)
        .map_err(|e| AppError::Message(e.to_string()))?;
    let mut results = Vec::new();
    let mut errors = Vec::new();
    for path in &image_paths {
        match crate::domain::image_service::import(&conn, &app, path) {
            Ok((image, is_duplicate)) => {
                if let Err(e) = crate::domain::image_service::relate_image_to_prompt(
                    &conn, &prompt.id, &image.id,
                ) {
                    errors.push(crate::domain::image_service::ImageImportError {
                        path: path.clone(),
                        message: format!("关联图像失败: {e}"),
                    });
                }
                results.push(crate::domain::image_service::ImageImportResult {
                    image,
                    is_duplicate,
                });
            }
            Err(e) => errors.push(crate::domain::image_service::ImageImportError {
                path: path.clone(),
                message: e.to_string(),
            }),
        }
    }
    Ok(CreatePromptWithImagesResult {
        prompt,
        results,
        errors,
    })
}

#[tauri::command]
#[specta::specta]
pub fn create_prompt(
    db: State<BkDb>,
    content: String,
    title: Option<String>,
) -> Result<prompt_service::Prompt, AppError> {
    let conn = db.0.lock().map_err(|e| AppError::Message(e.to_string()))?;
    prompt_service::create(&conn, &content, title).map_err(|e| AppError::Message(e.to_string()))
}

#[tauri::command]
#[specta::specta]
pub fn delete_prompt(db: State<BkDb>, id: String) -> Result<(), AppError> {
    let conn = db.0.lock().map_err(|e| AppError::Message(e.to_string()))?;
    prompt_service::remove(&conn, &id).map_err(|e| AppError::Message(e.to_string()))
}

/// 列出回收站中的提示词（已软删除）。
#[tauri::command]
#[specta::specta]
pub fn list_trashed_prompts(db: State<BkDb>) -> Result<Vec<prompt_service::Prompt>, AppError> {
    let conn = db.0.lock().map_err(|e| AppError::Message(e.to_string()))?;
    prompt_service::list_trashed(&conn).map_err(|e| AppError::Message(e.to_string()))
}

/// 恢复回收站中的提示词。
#[tauri::command]
#[specta::specta]
pub fn restore_prompt(db: State<BkDb>, id: String) -> Result<prompt_service::Prompt, AppError> {
    let conn = db.0.lock().map_err(|e| AppError::Message(e.to_string()))?;
    prompt_service::restore(&conn, &id)
        .map_err(|e| AppError::Message(e.to_string()))?
        .ok_or_else(|| "提示词不存在".into())
}

/// 彻底删除回收站中的提示词。
#[tauri::command]
#[specta::specta]
pub fn purge_prompt(db: State<BkDb>, id: String) -> Result<(), AppError> {
    let conn = db.0.lock().map_err(|e| AppError::Message(e.to_string()))?;
    prompt_service::purge(&conn, &id).map_err(|e| AppError::Message(e.to_string()))
}

/// 恢复全部回收站提示词，返回恢复数量。
#[tauri::command]
#[specta::specta]
pub fn restore_all_prompts(db: State<BkDb>) -> Result<usize, AppError> {
    let conn = db.0.lock().map_err(|e| AppError::Message(e.to_string()))?;
    prompt_service::restore_all(&conn).map_err(|e| AppError::Message(e.to_string()))
}

/// 清空提示词回收站（关联关系级联删除）。
#[tauri::command]
#[specta::specta]
pub fn empty_prompt_trash(db: State<BkDb>) -> Result<image_service::TrashBatchResult, AppError> {
    let conn = db.0.lock().map_err(|e| AppError::Message(e.to_string()))?;
    prompt_service::empty_trash(&conn)
        .map(|count| image_service::TrashBatchResult {
            count: count as usize,
            failures: 0,
        })
        .map_err(|e| AppError::Message(e.to_string()))
}

/// 返回每个提示词第一张关联（未删除）图像的缩略图磁盘路径：{promptId: absPath}，供卡片背景。
#[tauri::command]
#[specta::specta]
pub fn get_prompt_thumbs_map(
    app: tauri::AppHandle,
    db: State<BkDb>,
) -> Result<std::collections::HashMap<String, String>, AppError> {
    let conn = db.0.lock().map_err(|e| AppError::Message(e.to_string()))?;
    prompt_thumbs_map(&conn, &crate::infra::db::data_dir(&app))
        .map_err(|e| AppError::Message(e.to_string()))
}

/// 按关联顺序取每个提示词第一张**有缩略图**的图像：
/// thumbnail_path 为 NULL 的记录（如导入时无法解码的文件）自动跳过，
/// 让位给后续可用图像，且不会因 NULL 阻塞整个映射的构建。
pub(crate) fn prompt_thumbs_map(
    conn: &rusqlite::Connection,
    data_dir: &std::path::Path,
) -> rusqlite::Result<std::collections::HashMap<String, String>> {
    let mut stmt = conn.prepare(
        "SELECT prompt_id, thumbnail_path
         FROM (
            SELECT pir.prompt_id AS prompt_id, img.thumbnail_path AS thumbnail_path,
                   ROW_NUMBER() OVER (PARTITION BY pir.prompt_id ORDER BY pir.sort_order, pir.rowid) AS rn
            FROM prompt_image_relations pir
            JOIN images img ON img.id = pir.image_id
            WHERE img.is_deleted = 0 AND img.thumbnail_path IS NOT NULL
         )
         WHERE rn = 1",
    )?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
    let mut map: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for row in rows {
        let (pid, thumb_rel) = row?;
        map.insert(
            pid,
            data_dir.join(&thumb_rel).to_string_lossy().into_owned(),
        );
    }
    Ok(map)
}

/// 更新提示词详情字段（标题/内容/翻译/备注/收藏/安全）。
#[tauri::command]
#[specta::specta]
pub fn update_prompt_detail(
    db: State<BkDb>,
    id: String,
    title: Option<String>,
    content: Option<String>,
    content_translate: Option<String>,
    note: Option<String>,
    is_favorite: Option<bool>,
    is_safe: Option<bool>,
) -> Result<prompt_service::Prompt, AppError> {
    let conn = db.0.lock().map_err(|e| AppError::Message(e.to_string()))?;
    prompt_service::update_detail(
        &conn,
        &id,
        title,
        content,
        content_translate,
        note,
        is_favorite,
        is_safe,
    )
    .map_err(|e| AppError::Message(e.to_string()))?
    .ok_or_else(|| "提示词不存在".into())
}

/// 同步提示词的安全评级到其关联图像（修改提示词安全评级时联动一层，参考 pm 的双向联动）。
#[tauri::command]
#[specta::specta]
pub fn sync_prompt_safe_to_images(
    db: State<BkDb>,
    prompt_id: String,
    is_safe: bool,
) -> Result<usize, AppError> {
    let conn = db.0.lock().map_err(|e| AppError::Message(e.to_string()))?;
    conn.execute(
        "UPDATE images SET is_safe = ?1
         WHERE is_deleted = 0 AND id IN (
           SELECT image_id FROM prompt_image_relations WHERE prompt_id = ?2
         )",
        rusqlite::params![is_safe, prompt_id],
    )
    .map_err(|e| AppError::Message(e.to_string()))
}

/// 返回一个提示词关联的（未删除）图像列表（含缩略图与标签），供详情页网格展示。
#[tauri::command]
#[specta::specta]
pub fn get_prompt_related_images(
    app: tauri::AppHandle,
    db: State<BkDb>,
    id: String,
) -> Result<Vec<prompt_service::RelatedImage>, AppError> {
    let conn = db.0.lock().map_err(|e| AppError::Message(e.to_string()))?;
    prompt_service::list_related_images(&conn, &app, &id)
        .map_err(|e| AppError::Message(e.to_string()))
}

/// 设为首图：提示词详情图像右键，调整关联 sort_order 使该图排首位（对齐 pm）。
#[tauri::command]
#[specta::specta]
pub fn set_prompt_first_image(
    db: State<BkDb>,
    prompt_id: String,
    image_id: String,
) -> Result<(), AppError> {
    let conn = db.0.lock().map_err(|e| AppError::Message(e.to_string()))?;
    prompt_service::set_prompt_first_image(&conn, &prompt_id, &image_id)
}

/// 提示词详情解绑图像：从提示词移除一张图像的关联（与图像侧 remove_prompt_from_image 对称）。
#[tauri::command]
#[specta::specta]
pub fn remove_image_from_prompt(
    db: State<BkDb>,
    prompt_id: String,
    image_id: String,
) -> Result<(), AppError> {
    let conn = db.0.lock().map_err(|e| AppError::Message(e.to_string()))?;
    prompt_service::remove_image(&conn, &prompt_id, &image_id)
        .map_err(|e| AppError::Message(e.to_string()))
}

/// 为已存在的提示词导入外部图像并关联（复用导入 + 幂等关联），供详情页「从外界导入」。
#[tauri::command]
#[specta::specta]
pub fn add_images_to_prompt(
    app: tauri::AppHandle,
    db: State<BkDb>,
    prompt_id: String,
    image_paths: Vec<String>,
) -> Result<crate::domain::image_service::ImageImportBatchResult, AppError> {
    use crate::domain::image_service::{ImageImportError, ImageImportResult};
    let conn = db.0.lock().map_err(|e| AppError::Message(e.to_string()))?;
    let mut results = Vec::new();
    let mut errors = Vec::new();
    for path in &image_paths {
        match crate::domain::image_service::import(&conn, &app, path) {
            Ok((image, is_duplicate)) => {
                if let Err(e) = crate::domain::image_service::relate_image_to_prompt(
                    &conn, &prompt_id, &image.id,
                ) {
                    errors.push(ImageImportError {
                        path: path.clone(),
                        message: format!("关联图像失败: {e}"),
                    });
                } else {
                    results.push(ImageImportResult {
                        image,
                        is_duplicate,
                    });
                }
            }
            Err(e) => errors.push(ImageImportError {
                path: path.clone(),
                message: e.to_string(),
            }),
        }
    }
    Ok(crate::domain::image_service::ImageImportBatchResult { results, errors })
}

#[cfg(test)]
#[path = "prompt.test.rs"]
mod tests;
