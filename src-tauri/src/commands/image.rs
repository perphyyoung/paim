//! 图像命令层：薄适配，从 managed state 取连接，转调领域服务。
//! 路径 `commands::image::`，与提示词侧 `commands::prompt::` 对称。

use crate::commands::db_blocking;
use crate::domain::image_ops::{make_center_thumb, open_image};
use crate::domain::image_service::{
    self, Image, ImageCard, ImageImportBatchResult, ImageImportResult, ImageReplaceOutcome,
    LinkedPrompt, PaginatedImages,
};
use crate::domain::list_query::ListQuery;
use crate::domain::prompt_service;
use crate::domain::thumbnail_service::{
    self, ThumbnailEnsureResult, ThumbnailRebuildProgress, ThumbnailRebuildSummary,
};
use crate::infra::db::BkDb;
use crate::infra::error::AppError;

use tauri::{Manager, State};
use tauri_plugin_dialog::DialogExt;
use tauri_specta::Event;

/// 选择图像文件（支持多选），返回所选路径；与 import_images 构成上传弹窗的动作对：
/// select_images 选择文件 → import_images 导入入库。
/// e2e 测试缝（参考 pm 的主进程 dialog mock 模式）：debug 构建且设置了
/// PAIM_E2E_MOCK_IMAGE_PATHS（JSON 路径数组）时直接返回，绕过原生对话框；
/// 生产路径不受影响（该环境变量只在 e2e 启动的进程里存在）。
/// 长任务（阻塞等待用户选择），async + spawn_blocking。
#[tauri::command]
#[specta::specta]
pub async fn select_images(app: tauri::AppHandle) -> Result<Vec<String>, AppError> {
    #[cfg(debug_assertions)]
    if let Ok(mock) = std::env::var("PAIM_E2E_MOCK_IMAGE_PATHS") {
        if !mock.trim().is_empty() {
            return serde_json::from_str(&mock)
                .map_err(|e| AppError::Message(format!("e2e mock 路径解析失败: {e}")));
        }
    }

    tauri::async_runtime::spawn_blocking(move || {
        let picked = app
            .dialog()
            .file()
            .add_filter("图像", crate::domain::image_service::SUPPORTED_EXT)
            .blocking_pick_files();
        Ok(picked
            .unwrap_or_default()
            .iter()
            .map(|p| p.to_string())
            .collect())
    })
    .await
    .map_err(|e| AppError::Message(format!("文件选择任务执行失败: {e}")))?
}

/// 导入多张本地图像（可选关联到提示词内容），逐张容错返回结果与错误。
/// 附带提示词时只创建一条（首次导入成功时才创建，全部失败不留空提示词），
/// 本批全部图像关联到同一条——此前按图逐张新建，两张图附带同一提示词会生成两个提示词。
#[tauri::command]
#[specta::specta]
pub fn import_images(
    app: tauri::AppHandle,
    db: State<BkDb>,
    paths: Vec<String>,
    prompt: Option<String>,
) -> Result<ImageImportBatchResult, AppError> {
    let conn = db.0.lock().map_err(|e| AppError::Message(e.to_string()))?;
    let prompt = prompt
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let mut results = Vec::new();
    let mut errors = Vec::new();
    let mut prompt_id: Option<String> = None;
    for path in &paths {
        match image_service::import(&conn, &app, path) {
            Ok((image, is_duplicate)) => {
                if let Some(content) = &prompt {
                    let id = match &prompt_id {
                        Some(id) => id.clone(),
                        None => match prompt_service::create(&conn, content, None) {
                            Ok(created) => {
                                prompt_id = Some(created.id.clone());
                                created.id
                            }
                            Err(e) => {
                                errors.push(image_service::ImageImportError {
                                    path: path.clone(),
                                    message: format!("创建提示词失败: {e}"),
                                });
                                results.push(ImageImportResult {
                                    image,
                                    is_duplicate,
                                });
                                continue;
                            }
                        },
                    };
                    if let Err(e) = image_service::relate_image_to_prompt(&conn, &id, &image.id) {
                        errors.push(image_service::ImageImportError {
                            path: path.clone(),
                            message: format!("关联提示词失败: {e}"),
                        });
                    }
                }
                results.push(ImageImportResult {
                    image,
                    is_duplicate,
                });
            }
            Err(e) => errors.push(image_service::ImageImportError {
                path: path.clone(),
                message: e.to_string(),
            }),
        }
    }
    Ok(ImageImportBatchResult { results, errors })
}

/// 为上传弹窗提供源图预览缩略图：解码源图生成居中缩略图，写入 data 目录（已在 asset scope 内）。
#[tauri::command]
#[specta::specta]
pub fn get_source_thumbnail(app: tauri::AppHandle, source: String) -> Result<String, AppError> {
    use std::path::PathBuf;
    let src = PathBuf::from(&source);
    if !src.is_file() {
        return Err("源文件不存在".into());
    }
    let img = open_image(&src).map_err(|e| AppError::Message(format!("无法读取图像: {e}")))?;
    let thumb =
        make_center_thumb(&img).map_err(|e| AppError::Message(format!("生成缩略图失败: {e}")))?;

    let prev_dir = crate::infra::db::preview_dir(&app);
    std::fs::create_dir_all(&prev_dir).map_err(|e| AppError::Message(e.to_string()))?;
    // 以源文件路径哈希命名，重复选择复用
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    use std::hash::Hasher;
    hasher.write(source.as_bytes());
    let name = format!("pre_{:016x}.jpg", hasher.finish());
    let dest = prev_dir.join(&name);

    if !dest.exists() {
        thumb
            .save(&dest)
            .map_err(|e| AppError::Message(format!("保存预览失败: {e}")))?;
    }
    Ok(dest.to_string_lossy().to_string())
}

/// 替换图像：新图走标准入库管线，旧图软删并迁移关联（详情页右键）。
#[tauri::command]
#[specta::specta]
pub fn replace_image(
    app: tauri::AppHandle,
    db: State<BkDb>,
    old_id: String,
    source: String,
) -> Result<ImageReplaceOutcome, AppError> {
    let conn = db.0.lock().map_err(|e| AppError::Message(e.to_string()))?;
    image_service::replace_image(&conn, &app, &old_id, &source)
}

#[tauri::command]
#[specta::specta]
pub fn list_images(
    db: State<BkDb>,
    limit: Option<i64>,
    search: Option<String>,
    tag: Option<String>,
) -> Result<PaginatedImages, AppError> {
    let conn = db.0.lock().map_err(|e| AppError::Message(e.to_string()))?;
    let items = image_service::list(&conn, search.as_deref(), tag.as_deref(), limit)
        .map_err(|e| AppError::Message(e.to_string()))?;
    let total = image_service::count(&conn, search.as_deref(), tag.as_deref())
        .map_err(|e| AppError::Message(e.to_string()))?;
    Ok(PaginatedImages { items, total })
}

/// 主页分页列表：排序 / 搜索 / 标签筛选（含特殊标签）由后端完成，返回本页与符合条件的总数。
#[tauri::command]
#[specta::specta]
pub async fn list_images_page(
    db: State<'_, BkDb>,
    query: ListQuery,
) -> Result<PaginatedImages, AppError> {
    db_blocking(&db, move |conn| {
        image_service::list_page(conn, &query).map_err(|e| AppError::Message(e.to_string()))
    })
    .await
}

/// 同条件只取 id（全选 / 反选 / 批量操作用），条数封顶 `MAX_IDS`。
#[tauri::command]
#[specta::specta]
pub async fn list_image_ids(
    db: State<'_, BkDb>,
    query: ListQuery,
) -> Result<Vec<String>, AppError> {
    db_blocking(&db, move |conn| {
        image_service::list_ids(conn, &query).map_err(|e| AppError::Message(e.to_string()))
    })
    .await
}

/// 特殊标签命中数（基于全部未删除图像，不含搜索 / 标签条件）。
#[tauri::command]
#[specta::specta]
pub async fn image_special_tags_counts(
    db: State<'_, BkDb>,
) -> Result<std::collections::HashMap<String, i64>, AppError> {
    db_blocking(&db, move |conn| {
        image_service::special_tags_counts(conn).map_err(|e| AppError::Message(e.to_string()))
    })
    .await
}

/// 将一批已存在的图像关联到指定提示词（幂等，不重新导入文件），供详情页「从图像列表导入」。
#[tauri::command]
#[specta::specta]
pub fn relate_images_to_prompt(
    db: State<BkDb>,
    prompt_id: String,
    image_ids: Vec<String>,
) -> Result<usize, AppError> {
    let conn = db.0.lock().map_err(|e| AppError::Message(e.to_string()))?;
    let mut count = 0usize;
    for id in &image_ids {
        count += image_service::relate_image_to_prompt(&conn, &prompt_id, id)
            .map_err(|e| AppError::Message(e.to_string()))? as usize;
    }
    Ok(count)
}

/// 返回回收站中的图像（已软删除），与 list_trashed_prompts 对称。
#[tauri::command]
#[specta::specta]
pub fn list_trashed_images(db: State<BkDb>) -> Result<Vec<ImageCard>, AppError> {
    let conn = db.0.lock().map_err(|e| AppError::Message(e.to_string()))?;
    image_service::list_trashed(&conn).map_err(|e| AppError::Message(e.to_string()))
}

#[tauri::command]
#[specta::specta]
pub fn delete_image(db: State<BkDb>, id: String) -> Result<Image, AppError> {
    let conn = db.0.lock().map_err(|e| AppError::Message(e.to_string()))?;
    image_service::soft_delete(&conn, &id)
        .map_err(|e| AppError::Message(e.to_string()))?
        .ok_or_else(|| "图像不存在".into())
}

#[tauri::command]
#[specta::specta]
pub fn restore_image(db: State<BkDb>, id: String) -> Result<Image, AppError> {
    let conn = db.0.lock().map_err(|e| AppError::Message(e.to_string()))?;
    image_service::restore(&conn, &id)
        .map_err(|e| AppError::Message(e.to_string()))?
        .ok_or_else(|| "图像不存在".into())
}

#[tauri::command]
#[specta::specta]
pub fn purge_image(app: tauri::AppHandle, db: State<BkDb>, id: String) -> Result<(), AppError> {
    let conn = db.0.lock().map_err(|e| AppError::Message(e.to_string()))?;
    image_service::purge(&conn, &app, &id).map_err(|e| AppError::Message(e.to_string()))
}

/// 恢复全部回收站图像，返回恢复数量。
#[tauri::command]
#[specta::specta]
pub fn restore_all_images(db: State<BkDb>) -> Result<usize, AppError> {
    let conn = db.0.lock().map_err(|e| AppError::Message(e.to_string()))?;
    image_service::restore_all(&conn).map_err(|e| AppError::Message(e.to_string()))
}

/// 清空图像回收站（逐项彻底删除，含磁盘文件），逐项容错。
#[tauri::command]
#[specta::specta]
pub fn empty_image_trash(
    app: tauri::AppHandle,
    db: State<BkDb>,
) -> Result<image_service::TrashBatchResult, AppError> {
    let conn = db.0.lock().map_err(|e| AppError::Message(e.to_string()))?;
    Ok(image_service::empty_trash(&conn, &app))
}

/// 返回单张图像详情。
#[tauri::command]
#[specta::specta]
pub fn get_image_detail(db: State<BkDb>, id: String) -> Result<Image, AppError> {
    let conn = db.0.lock().map_err(|e| AppError::Message(e.to_string()))?;
    image_service::get_by_id(&conn, &id)
        .map_err(|e| AppError::Message(e.to_string()))?
        .ok_or_else(|| "图像不存在".into())
}

/// 返回图像原图磁盘路径，前端配合 convertFileSrc 加载（详情页大图使用）。
#[tauri::command]
#[specta::specta]
pub fn get_image_src(
    app: tauri::AppHandle,
    db: State<BkDb>,
    id: String,
) -> Result<String, AppError> {
    let conn = db.0.lock().map_err(|e| AppError::Message(e.to_string()))?;
    let rel: Option<String> = conn
        .query_row(
            "SELECT relative_path FROM images WHERE id = ?1",
            rusqlite::params![id],
            |r| r.get(0),
        )
        .map_err(|e| AppError::Message(e.to_string()))?;

    let Some(rel) = rel else {
        return Err("原图不存在".into());
    };
    Ok(crate::infra::db::data_dir(&app)
        .join(&rel)
        .to_string_lossy()
        .into_owned())
}

/// 更新图像详情字段（文件名、备注、收藏、安全评级）。
#[tauri::command]
#[specta::specta]
pub fn update_image_detail(
    db: State<BkDb>,
    id: String,
    file_name: Option<String>,
    note: Option<String>,
    is_favorite: Option<bool>,
    is_safe: Option<bool>,
) -> Result<Image, AppError> {
    let conn = db.0.lock().map_err(|e| AppError::Message(e.to_string()))?;
    image_service::update_detail(
        &conn,
        &id,
        file_name.as_deref(),
        note.as_deref(),
        is_favorite,
        is_safe,
    )
    .map_err(|e| AppError::Message(e.to_string()))?
    .ok_or_else(|| "图像不存在".into())
}

/// 同步图像的安全评级到其关联提示词（修改图像安全评级时联动一层，参考 pm 的双向联动）。
#[tauri::command]
#[specta::specta]
pub fn sync_image_safe_to_prompts(
    db: State<BkDb>,
    image_id: String,
    is_safe: bool,
) -> Result<usize, AppError> {
    let conn = db.0.lock().map_err(|e| AppError::Message(e.to_string()))?;
    conn.execute(
        "UPDATE prompts SET is_safe = ?1
         WHERE is_deleted = 0 AND id IN (
           SELECT prompt_id FROM prompt_image_relations WHERE image_id = ?2
         )",
        rusqlite::params![is_safe, image_id],
    )
    .map_err(|e| AppError::Message(e.to_string()))
}

/// 图像详情解绑提示词：从图像移除一条提示词关联（与提示词侧 remove_image_from_prompt 对称）。
#[tauri::command]
#[specta::specta]
pub fn remove_prompt_from_image(
    db: State<BkDb>,
    image_id: String,
    prompt_id: String,
) -> Result<(), AppError> {
    let conn = db.0.lock().map_err(|e| AppError::Message(e.to_string()))?;
    prompt_service::remove_image(&conn, &prompt_id, &image_id)
        .map_err(|e| AppError::Message(e.to_string()))
}

/// 返回非删除图像到其关联提示词内容的映射：{imageId: [content,...]}，供卡片 row2 显示。
#[tauri::command]
#[specta::specta]
pub async fn get_image_prompts_map(
    db: State<'_, BkDb>,
) -> Result<std::collections::HashMap<String, Vec<String>>, AppError> {
    db_blocking(&db, move |conn| {
        let mut stmt = conn
            .prepare(
                "SELECT img.id, pr.content
             FROM images img
             JOIN prompt_image_relations pir ON pir.image_id = img.id
             JOIN prompts pr ON pr.id = pir.prompt_id
             WHERE img.is_deleted = 0 AND pr.is_deleted = 0
             ORDER BY pr.created_at",
            )
            .map_err(|e| AppError::Message(e.to_string()))?;
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
            .map_err(|e| AppError::Message(e.to_string()))?;
        let mut map: std::collections::HashMap<String, Vec<String>> =
            std::collections::HashMap::new();
        for row in rows {
            let (img_id, content) = row.map_err(|e| AppError::Message(e.to_string()))?;
            map.entry(img_id).or_default().push(content);
        }
        Ok(map)
    })
    .await
}

/// 返回单张图像关联的提示词列表（含标题/内容/翻译/备注/标签），供详情页左侧展示。
#[tauri::command]
#[specta::specta]
pub fn get_image_related_prompts(
    db: State<BkDb>,
    id: String,
) -> Result<Vec<LinkedPrompt>, AppError> {
    let conn = db.0.lock().map_err(|e| AppError::Message(e.to_string()))?;
    image_service::list_related_prompts(&conn, &id).map_err(|e| AppError::Message(e.to_string()))
}

/// 为指定图像新建提示词并关联（复用 create_prompt + relate），供图像详情「新建提示词」。
#[tauri::command]
#[specta::specta]
pub fn create_prompt_for_image(
    db: State<BkDb>,
    content: String,
    image_id: String,
) -> Result<(), AppError> {
    let conn = db.0.lock().map_err(|e| AppError::Message(e.to_string()))?;
    let prompt = prompt_service::create(&conn, &content, None)
        .map_err(|e| AppError::Message(e.to_string()))?;
    image_service::relate_image_to_prompt(&conn, &prompt.id, &image_id)
        .map_err(|e| AppError::Message(e.to_string()))?;
    Ok(())
}

/// 设置页「重建缩略图」：扫描全部图像，补齐丢失的缩略图文件并回写路径，
/// 进度经 thumbnail-rebuild-progress 事件推送。重 IO 长任务，async + spawn_blocking。
#[tauri::command]
#[specta::specta]
pub async fn rebuild_thumbnails(
    app: tauri::AppHandle,
) -> Result<ThumbnailRebuildSummary, AppError> {
    tauri::async_runtime::spawn_blocking(move || {
        let data_dir = crate::infra::db::data_dir(&app);
        let thumbs_root = crate::infra::db::thumbnails_dir(&app);
        let bk = app.state::<BkDb>();
        let conn = bk.0.lock().map_err(|e| e.to_string())?;
        thumbnail_service::rebuild_all(&data_dir, &thumbs_root, &conn, |done, total, file_name| {
            let _ = ThumbnailRebuildProgress {
                current: done,
                total,
                file_name: file_name.to_string(),
            }
            .emit(&app);
        })
    })
    .await
    .map_err(|e| AppError::Message(format!("重建缩略图任务执行失败: {e}")))?
    .map_err(AppError::from)
}

/// 懒自愈：批量校验指定图像的缩略图文件，缺失且原图存在时按需生成并回写。
/// 正常路径仅 N 次文件存在性检查；与查询命令同走 spawn_blocking，慢盘时不冻结 UI。
#[tauri::command]
#[specta::specta]
pub async fn ensure_image_thumbnails(
    app: tauri::AppHandle,
    db: State<'_, BkDb>,
    ids: Vec<String>,
) -> Result<ThumbnailEnsureResult, AppError> {
    db_blocking(&db, move |conn| {
        thumbnail_service::ensure(
            &crate::infra::db::data_dir(&app),
            &crate::infra::db::thumbnails_dir(&app),
            conn,
            &ids,
        )
        .map_err(AppError::from)
    })
    .await
}
