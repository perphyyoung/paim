//! 提示词命令层：薄适配，从 managed state 取连接，转调领域服务。
//! 路径 `commands::prompt::`，与图像侧 `commands::image::` 对称。

use crate::domain::image_service;
use crate::domain::list_query::ListQuery;
use crate::domain::prompt_merge;
use crate::domain::prompt_service;
use crate::domain::similarity_service;
use crate::domain::thumbnail_service;
use crate::infra::db::{self, BkDb};
use crate::infra::error::AppError;

use serde::Serialize;
use tauri::State;

#[tauri::command]
#[specta::specta]
pub async fn list_prompts(
    db: State<'_, BkDb>,
) -> Result<Vec<prompt_service::PromptCard>, AppError> {
    db::blocking(&db, move |conn| {
        prompt_service::list(conn).map_err(|e| AppError::Message(e.to_string()))
    })
    .await
}

/// 主页分页列表：排序 / 搜索 / 标签筛选（含特殊标签）由后端完成，返回本页与符合条件的总数。
#[tauri::command]
#[specta::specta]
pub async fn list_prompts_page(
    db: State<'_, BkDb>,
    query: ListQuery,
) -> Result<prompt_service::PaginatedPrompts, AppError> {
    db::blocking(&db, move |conn| {
        prompt_service::list_page(conn, &query).map_err(|e| AppError::Message(e.to_string()))
    })
    .await
}

/// 同条件只取 id（全选 / 反选 / 批量操作用），条数封顶 `MAX_IDS`。
#[tauri::command]
#[specta::specta]
pub async fn list_prompt_ids(
    db: State<'_, BkDb>,
    query: ListQuery,
) -> Result<Vec<String>, AppError> {
    db::blocking(&db, move |conn| {
        prompt_service::list_ids(conn, &query).map_err(|e| AppError::Message(e.to_string()))
    })
    .await
}

/// 按 id 列表取卡片投影（顺序与入参一致，缺失 / 已软删的跳过）。
/// 供「按 id 渲染卡片」的场景使用，如相似度检索结果。
#[tauri::command]
#[specta::specta]
pub async fn prompt_cards_by_ids(
    db: State<'_, BkDb>,
    ids: Vec<String>,
) -> Result<Vec<prompt_service::PromptCard>, AppError> {
    db::blocking(&db, move |conn| {
        prompt_service::cards_by_ids(conn, &ids).map_err(|e| AppError::Message(e.to_string()))
    })
    .await
}

/// 特殊标签命中数（基于全部未删除提示词，不含搜索 / 标签条件）。
#[tauri::command]
#[specta::specta]
pub async fn prompt_special_tags_counts(
    db: State<'_, BkDb>,
) -> Result<std::collections::HashMap<String, i64>, AppError> {
    db::blocking(&db, move |conn| {
        prompt_service::special_tags_counts(conn).map_err(|e| AppError::Message(e.to_string()))
    })
    .await
}

#[derive(Debug, Serialize, Clone, specta::Type)]
pub struct CreatePromptWithImagesResult {
    pub prompt: prompt_service::Prompt,
    pub results: Vec<crate::domain::image_service::ImageImportResult>,
    pub errors: Vec<crate::domain::image_service::ImageImportError>,
}

/// 新建提示词（内容必需）；image_paths 非空时上传并关联到该提示词。
/// 与 `import_images` 同口径：提示词入库是短锁，图片走「无锁预处理 + 短锁入库 / 关联」。
#[tauri::command]
#[specta::specta]
pub async fn create_prompt_with_images(
    db: State<'_, BkDb>,
    app: tauri::AppHandle,
    content: String,
    title: Option<String>,
    image_paths: Vec<String>,
) -> Result<CreatePromptWithImagesResult, AppError> {
    let prompt = db::blocking(&db, move |conn| {
        prompt_service::create(conn, &content, title).map_err(AppError::from)
    })
    .await?;
    let mut results = Vec::new();
    let mut errors = Vec::new();
    for path in &image_paths {
        match crate::commands::image::import_one(&db, &app, path).await {
            Ok((image, is_duplicate)) => {
                let prompt_id = prompt.id.clone();
                let image_id = image.id.clone();
                if let Err(e) = db::blocking(&db, move |conn| {
                    image_service::relate_image_to_prompt(conn, &prompt_id, &image_id)
                        .map_err(AppError::from)
                })
                .await
                {
                    errors.push(image_service::ImageImportError {
                        path: path.clone(),
                        message: format!("关联图像失败: {e}"),
                    });
                }
                results.push(image_service::ImageImportResult {
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
    Ok(CreatePromptWithImagesResult {
        prompt,
        results,
        errors,
    })
}

#[tauri::command]
#[specta::specta]
pub async fn create_prompt(
    db: State<'_, BkDb>,
    content: String,
    title: Option<String>,
) -> Result<prompt_service::Prompt, AppError> {
    db::blocking(&db, move |conn| {
        prompt_service::create(conn, &content, title).map_err(|e| AppError::Message(e.to_string()))
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn delete_prompt(db: State<'_, BkDb>, id: String) -> Result<(), AppError> {
    db::blocking(&db, move |conn| {
        prompt_service::remove(conn, &id).map_err(|e| AppError::Message(e.to_string()))
    })
    .await
}

/// 合并两条提示词前的预览（差异由前端做词级对齐，这里给两侧原文与并集口径）。
#[tauri::command]
#[specta::specta]
pub async fn preview_merge_prompts(
    db: State<'_, BkDb>,
    a_id: String,
    b_id: String,
) -> Result<prompt_merge::MergePromptsPreview, AppError> {
    db::blocking(&db, move |conn| {
        prompt_merge::preview(conn, &a_id, &b_id).map_err(|e| AppError::Message(e.to_string()))
    })
    .await
}

/// 合并两条提示词：新建一条（标题用新 id、译文空、note 合并），图像/标签取并集，原两条软删。
#[tauri::command]
#[specta::specta]
pub async fn merge_prompts(
    db: State<'_, BkDb>,
    a_id: String,
    b_id: String,
    content: String,
) -> Result<prompt_service::Prompt, AppError> {
    db::blocking(&db, move |conn| {
        prompt_merge::merge(conn, &a_id, &b_id, &content)
            .map_err(|e| AppError::Message(e.to_string()))
    })
    .await
}

/// 列出回收站中的提示词（已软删除）。
#[tauri::command]
#[specta::specta]
pub async fn list_trashed_prompts(
    db: State<'_, BkDb>,
) -> Result<Vec<prompt_service::PromptCard>, AppError> {
    db::blocking(&db, move |conn| {
        prompt_service::list_trashed(conn).map_err(|e| AppError::Message(e.to_string()))
    })
    .await
}

/// 恢复回收站中的提示词。
#[tauri::command]
#[specta::specta]
pub async fn restore_prompt(
    db: State<'_, BkDb>,
    id: String,
) -> Result<prompt_service::Prompt, AppError> {
    db::blocking(&db, move |conn| {
        prompt_service::restore(conn, &id)
            .map_err(|e| AppError::Message(e.to_string()))?
            .ok_or_else(|| "提示词不存在".into())
    })
    .await
}

/// 彻底删除回收站中的提示词。
#[tauri::command]
#[specta::specta]
pub async fn purge_prompt(db: State<'_, BkDb>, id: String) -> Result<(), AppError> {
    db::blocking(&db, move |conn| {
        prompt_service::purge(conn, &id).map_err(|e| AppError::Message(e.to_string()))
    })
    .await
}

/// 恢复全部回收站提示词，返回恢复数量。
#[tauri::command]
#[specta::specta]
pub async fn restore_all_prompts(db: State<'_, BkDb>) -> Result<usize, AppError> {
    db::blocking(&db, move |conn| {
        prompt_service::restore_all(conn).map_err(|e| AppError::Message(e.to_string()))
    })
    .await
}

/// 清空提示词回收站（关联关系级联删除）。
#[tauri::command]
#[specta::specta]
pub async fn empty_prompt_trash(
    db: State<'_, BkDb>,
) -> Result<image_service::TrashBatchResult, AppError> {
    db::blocking(&db, move |conn| {
        prompt_service::empty_trash(conn)
            .map(|count| image_service::TrashBatchResult {
                count: count as usize,
                failures: 0,
            })
            .map_err(|e| AppError::Message(e.to_string()))
    })
    .await
}

/// 返回给定提示词第一张关联（未删除）图像的缩略图**相对路径**：{promptId: relPath}，供卡片背景。
/// 与图像列表明细里的 `thumbnail_path` 同语义，由前端拼数据目录；空列表直接返回空，避免拼出空 IN。
#[tauri::command]
#[specta::specta]
pub async fn get_prompt_thumbs(
    db: State<'_, BkDb>,
    ids: Vec<String>,
) -> Result<std::collections::HashMap<String, String>, AppError> {
    db::blocking(&db, move |conn| {
        prompt_service::thumbs_for(conn, &ids).map_err(|e| AppError::Message(e.to_string()))
    })
    .await
}

/// 回收站提示词卡片背景：{promptId: relPath}。与 `get_prompt_thumbs` 的差异是
/// 纳入已软删的关联图像（回收站特殊语义：关系还在就显示，哪怕图像也在图像回收站里）。
#[tauri::command]
#[specta::specta]
pub async fn get_trashed_prompt_thumbs(
    db: State<'_, BkDb>,
    ids: Vec<String>,
) -> Result<std::collections::HashMap<String, String>, AppError> {
    db::blocking(&db, move |conn| {
        prompt_service::thumbs_for_trashed(conn, &ids).map_err(|e| AppError::Message(e.to_string()))
    })
    .await
}

/// 提示词卡片背景懒自愈：可见窗口稳定后按提示词校验其关联图像的缩略图，缺图按需生成。
/// 返回与图像侧对称的 ThumbnailEnsureResult（fixed + missing）。
/// 三阶段：**短锁取规划 → 无锁生成 → 短锁回写**（生成期间不持锁，见 docs/lessons.md 第 28 节）。
#[tauri::command]
#[specta::specta]
pub async fn ensure_prompt_thumbnails(
    app: tauri::AppHandle,
    db: State<'_, BkDb>,
    ids: Vec<String>,
) -> Result<thumbnail_service::ThumbnailEnsureResult, AppError> {
    let data_dir = crate::infra::db::data_dir(&app);
    let thumbs_root = crate::infra::db::thumbnails_dir(&app);
    let plan_ids = ids.clone();
    let plan = db::blocking(&db, move |conn| {
        prompt_service::thumb_plan(conn, &plan_ids).map_err(AppError::Message)
    })
    .await?;

    let targets = plan.targets().to_vec();
    let outcome = crate::infra::task::spawn_blocking("缩略图自愈", move || {
        // 懒自愈只处理一屏项数：顺序执行，不抬内存峰值
        Ok(thumbnail_service::build_missing(
            &data_dir,
            &thumbs_root,
            &targets,
            1,
            |_, _, _| {},
        ))
    })
    .await?;

    db::blocking(&db, move |conn| {
        prompt_service::thumb_apply(conn, &ids, plan, &outcome.fixed).map_err(AppError::Message)
    })
    .await
}

/// 提示词内容的向量规范化形态（纯本地计算，不依赖 embedding 服务）。
/// 前端保存编辑前用它比对新旧内容：规范化后相同 = 仅排版调整（空白 / 标点旁空白），
/// 保存不影响向量；不同则为实质修改，需用户确认后保存。
#[tauri::command]
#[specta::specta]
pub fn canonicalize_prompt_text(content: String) -> String {
    similarity_service::canonical_text(&content)
}

/// 更新提示词详情字段（标题/内容/翻译/备注/收藏/安全）。
/// 保存路径**不依赖 embedding 服务**（llama.cpp 开不开都一样），会卡的只有取连接锁与写库，
/// 故成对埋点 + 慢告警（见 `commands::timed`）：卡住时「完成」永不落盘，即第一现场。
#[tauri::command]
#[specta::specta]
pub async fn update_prompt_detail(
    db: State<'_, BkDb>,
    id: String,
    title: Option<String>,
    content: Option<String>,
    content_translate: Option<String>,
    note: Option<String>,
    is_favorite: Option<bool>,
    is_safe: Option<bool>,
) -> Result<prompt_service::Prompt, AppError> {
    let _timed = crate::commands::timed("update_prompt_detail");
    // 只记「哪些字段要写」，不把提示词正文灌进日志
    crate::log_debug!(
        "update_prompt_detail 字段 id={id} title={} content={} translate={} note={} favorite={} safe={}",
        title.is_some(),
        content.is_some(),
        content_translate.is_some(),
        note.is_some(),
        is_favorite.is_some(),
        is_safe.is_some()
    );
    db::blocking(&db, move |conn| {
        prompt_service::update_detail(
            conn,
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
    })
    .await
}

/// 同步提示词的安全评级到其关联图像（修改提示词安全评级时联动一层，参考 pm 的双向联动）。
#[tauri::command]
#[specta::specta]
pub async fn sync_prompt_safe_to_images(
    db: State<'_, BkDb>,
    prompt_id: String,
    is_safe: bool,
) -> Result<usize, AppError> {
    db::blocking(&db, move |conn| {
        conn.execute(
            "UPDATE images SET is_safe = ?1
             WHERE is_deleted = 0 AND id IN (
               SELECT image_id FROM prompt_image_relations WHERE prompt_id = ?2
             )",
            rusqlite::params![is_safe, prompt_id],
        )
        .map_err(|e| AppError::Message(e.to_string()))
    })
    .await
}

/// 返回一个提示词关联的（未删除）图像列表（含缩略图与标签），供详情页网格展示。
#[tauri::command]
#[specta::specta]
pub async fn get_prompt_related_images(
    app: tauri::AppHandle,
    db: State<'_, BkDb>,
    id: String,
) -> Result<Vec<prompt_service::RelatedImage>, AppError> {
    db::blocking(&db, move |conn| {
        prompt_service::list_related_images(conn, &app, &id)
            .map_err(|e| AppError::Message(e.to_string()))
    })
    .await
}

/// 设为首图：提示词详情图像右键，调整关联 sort_order 使该图排首位（对齐 pm）。
#[tauri::command]
#[specta::specta]
pub async fn set_prompt_first_image(
    db: State<'_, BkDb>,
    prompt_id: String,
    image_id: String,
) -> Result<(), AppError> {
    db::blocking(&db, move |conn| {
        prompt_service::set_prompt_first_image(conn, &prompt_id, &image_id)
    })
    .await
}

/// 提示词详情解绑图像：从提示词移除一张图像的关联（与图像侧 remove_prompt_from_image 对称）。
#[tauri::command]
#[specta::specta]
pub async fn remove_image_from_prompt(
    db: State<'_, BkDb>,
    prompt_id: String,
    image_id: String,
) -> Result<(), AppError> {
    db::blocking(&db, move |conn| {
        prompt_service::remove_image(conn, &prompt_id, &image_id)
            .map_err(|e| AppError::Message(e.to_string()))
    })
    .await
}

/// 为已存在的提示词导入外部图像并关联（复用导入 + 幂等关联），供详情页「从外界导入」。
/// 图片走「无锁预处理 + 短锁入库 / 关联」，与 `import_images` 同一条管线。
#[tauri::command]
#[specta::specta]
pub async fn add_images_to_prompt(
    app: tauri::AppHandle,
    db: State<'_, BkDb>,
    prompt_id: String,
    image_paths: Vec<String>,
) -> Result<crate::domain::image_service::ImageImportBatchResult, AppError> {
    use crate::domain::image_service::{ImageImportError, ImageImportResult};
    let mut results = Vec::new();
    let mut errors = Vec::new();
    for path in &image_paths {
        match crate::commands::image::import_one(&db, &app, path).await {
            Ok((image, is_duplicate)) => {
                let pid = prompt_id.clone();
                let image_id = image.id.clone();
                if let Err(e) = db::blocking(&db, move |conn| {
                    image_service::relate_image_to_prompt(conn, &pid, &image_id)
                        .map_err(AppError::from)
                })
                .await
                {
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
