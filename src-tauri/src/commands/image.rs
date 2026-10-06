//! 图像命令层：薄适配，从 managed state 取连接，转调领域服务。
//! 路径 `commands::image::`，与提示词侧 `commands::prompt::` 对称。

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
use crate::infra::db::{self, BkDb};
use crate::infra::error::AppError;

use rusqlite::OptionalExtension;
use tauri::State;
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

/// 单张导入的编排（三个导入入口共用）：无锁哈希 → 短锁去重 → 无锁解码/落盘/缩略图 → 短锁入库。
/// 锁内只有点查询与单条 INSERT；解码、复制、缩略图都在锁外（见 docs/lessons.md 第 28 节）。
pub(crate) async fn import_one(
    db: &State<'_, BkDb>,
    app: &tauri::AppHandle,
    source: &str,
) -> Result<(Image, bool), AppError> {
    let src = source.to_string();
    let (path, md5) = crate::infra::task::spawn_blocking("导入", move || {
        image_service::prepare_source(&src).map_err(AppError::from)
    })
    .await?;

    let md5_for_dedupe = md5.clone();
    if let Some(existing) = db::blocking(db, move |conn| {
        image_service::dedupe_by_md5(conn, &md5_for_dedupe).map_err(AppError::from)
    })
    .await?
    {
        return Ok((existing, true));
    }

    let images_dir = crate::infra::db::images_dir(app);
    let thumbs_dir = crate::infra::db::thumbnails_dir(app);
    let prepared = crate::infra::task::spawn_blocking("导入", move || {
        image_service::prepare_files(&images_dir, &thumbs_dir, &path, &md5).map_err(AppError::from)
    })
    .await?;

    db::blocking(db, move |conn| {
        image_service::insert_prepared(conn, prepared).map_err(AppError::from)
    })
    .await
}

/// 导入多张本地图像（可选关联到提示词内容），逐张容错返回结果与错误。
/// 附带提示词时只创建一条（首次导入成功时才创建，全部失败不留空提示词），
/// 本批全部图像关联到同一条——此前按图逐张新建，两张图附带同一提示词会生成两个提示词。
#[tauri::command]
#[specta::specta]
pub async fn import_images(
    app: tauri::AppHandle,
    db: State<'_, BkDb>,
    paths: Vec<String>,
    prompt: Option<String>,
) -> Result<ImageImportBatchResult, AppError> {
    let prompt = prompt
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let mut results = Vec::new();
    let mut errors = Vec::new();
    let mut prompt_id: Option<String> = None;
    for path in &paths {
        match import_one(&db, &app, path).await {
            Ok((image, is_duplicate)) => {
                if let Some(content) = &prompt {
                    let id = match &prompt_id {
                        Some(id) => id.clone(),
                        None => {
                            let content = content.clone();
                            match db::blocking(&db, move |conn| {
                                prompt_service::create(conn, &content, None).map_err(AppError::from)
                            })
                            .await
                            {
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
                            }
                        }
                    };
                    let image_id = image.id.clone();
                    if let Err(e) = db::blocking(&db, move |conn| {
                        image_service::relate_image_to_prompt(conn, &id, &image_id)
                            .map_err(AppError::from)
                    })
                    .await
                    {
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
/// 解码 + 编码是重活，放阻塞池（命令层不在主线程上做编解码）。
#[tauri::command]
#[specta::specta]
pub async fn get_source_thumbnail(
    app: tauri::AppHandle,
    source: String,
) -> Result<String, AppError> {
    crate::infra::task::spawn_blocking("预览生成", move || {
        let src = std::path::PathBuf::from(&source);
        if !src.is_file() {
            return Err("源文件不存在".into());
        }
        let img = open_image(&src).map_err(|e| AppError::Message(format!("无法读取图像: {e}")))?;
        let thumb = make_center_thumb(&img)
            .map_err(|e| AppError::Message(format!("生成缩略图失败: {e}")))?;

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
    })
    .await
}

/// 替换图像：新图走标准入库管线，旧图软删并迁移关联（详情页右键）。
/// 编排与导入同口径：**短锁校验旧图 → 无锁导入新图 → 短锁迁移**，复制与缩略图期间不持锁。
#[tauri::command]
#[specta::specta]
pub async fn replace_image(
    app: tauri::AppHandle,
    db: State<'_, BkDb>,
    old_id: String,
    source: String,
) -> Result<ImageReplaceOutcome, AppError> {
    let old_for_check = old_id.clone();
    db::blocking(&db, move |conn| {
        image_service::replace_ensure_old(conn, &old_for_check)
    })
    .await?;

    let (new_img, is_duplicate) = import_one(&db, &app, &source).await?;
    if is_duplicate && new_img.id == old_id {
        return Ok(ImageReplaceOutcome::SameImage);
    }

    db::blocking(&db, move |conn| {
        image_service::replace_commit(conn, &old_id, &new_img)
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn list_images(
    db: State<'_, BkDb>,
    limit: Option<i64>,
    search: Option<String>,
    tag: Option<String>,
) -> Result<PaginatedImages, AppError> {
    db::blocking(&db, move |conn| {
        let items = image_service::list(conn, search.as_deref(), tag.as_deref(), limit)
            .map_err(|e| AppError::Message(e.to_string()))?;
        let total = image_service::count(conn, search.as_deref(), tag.as_deref())
            .map_err(|e| AppError::Message(e.to_string()))?;
        Ok(PaginatedImages { items, total })
    })
    .await
}

/// 主页分页列表：排序 / 搜索 / 标签筛选（含特殊标签）由后端完成，返回本页与符合条件的总数。
#[tauri::command]
#[specta::specta]
pub async fn list_images_page(
    db: State<'_, BkDb>,
    query: ListQuery,
) -> Result<PaginatedImages, AppError> {
    db::blocking(&db, move |conn| {
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
    db::blocking(&db, move |conn| {
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
    db::blocking(&db, move |conn| {
        image_service::special_tags_counts(conn).map_err(|e| AppError::Message(e.to_string()))
    })
    .await
}

/// 将一批已存在的图像关联到指定提示词（幂等，不重新导入文件），供详情页「从图像列表导入」。
#[tauri::command]
#[specta::specta]
pub async fn relate_images_to_prompt(
    db: State<'_, BkDb>,
    prompt_id: String,
    image_ids: Vec<String>,
) -> Result<usize, AppError> {
    db::blocking(&db, move |conn| {
        let mut count = 0usize;
        for id in &image_ids {
            count += image_service::relate_image_to_prompt(conn, &prompt_id, id)
                .map_err(|e| AppError::Message(e.to_string()))? as usize;
        }
        Ok(count)
    })
    .await
}

/// 返回回收站中的图像（已软删除），与 list_trashed_prompts 对称。
#[tauri::command]
#[specta::specta]
pub async fn list_trashed_images(db: State<'_, BkDb>) -> Result<Vec<ImageCard>, AppError> {
    db::blocking(&db, move |conn| {
        image_service::list_trashed(conn).map_err(|e| AppError::Message(e.to_string()))
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn delete_image(db: State<'_, BkDb>, id: String) -> Result<Image, AppError> {
    db::blocking(&db, move |conn| {
        image_service::soft_delete(conn, &id)
            .map_err(|e| AppError::Message(e.to_string()))?
            .ok_or_else(|| "图像不存在".into())
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn restore_image(db: State<'_, BkDb>, id: String) -> Result<Image, AppError> {
    db::blocking(&db, move |conn| {
        image_service::restore(conn, &id)
            .map_err(|e| AppError::Message(e.to_string()))?
            .ok_or_else(|| "图像不存在".into())
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn purge_image(
    app: tauri::AppHandle,
    db: State<'_, BkDb>,
    id: String,
) -> Result<(), AppError> {
    db::blocking(&db, move |conn| {
        image_service::purge(conn, &app, &id).map_err(|e| AppError::Message(e.to_string()))
    })
    .await
}

/// 恢复全部回收站图像，返回恢复数量。
#[tauri::command]
#[specta::specta]
pub async fn restore_all_images(db: State<'_, BkDb>) -> Result<usize, AppError> {
    db::blocking(&db, move |conn| {
        image_service::restore_all(conn).map_err(|e| AppError::Message(e.to_string()))
    })
    .await
}

/// 清空图像回收站（逐项彻底删除，含磁盘文件），逐项容错。
#[tauri::command]
#[specta::specta]
pub async fn empty_image_trash(
    app: tauri::AppHandle,
    db: State<'_, BkDb>,
) -> Result<image_service::TrashBatchResult, AppError> {
    db::blocking(&db, move |conn| Ok(image_service::empty_trash(conn, &app))).await
}

/// 返回单张图像详情。
#[tauri::command]
#[specta::specta]
pub async fn get_image_detail(db: State<'_, BkDb>, id: String) -> Result<Image, AppError> {
    db::blocking(&db, move |conn| {
        image_service::get_by_id(conn, &id)
            .map_err(|e| AppError::Message(e.to_string()))?
            .ok_or_else(|| "图像不存在".into())
    })
    .await
}

/// 返回图像原图磁盘路径，前端配合 convertFileSrc 加载（详情页大图使用）。
#[tauri::command]
#[specta::specta]
pub async fn get_image_src(
    app: tauri::AppHandle,
    db: State<'_, BkDb>,
    id: String,
) -> Result<String, AppError> {
    db::blocking(&db, move |conn| {
        let row: Option<(String, String)> = conn
            .query_row(
                "SELECT relative_path, file_name FROM images WHERE id = ?1",
                rusqlite::params![id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(|e| AppError::Message(e.to_string()))?;

        let Some((rel, file_name)) = row else {
            return Err("原图不存在".into());
        };
        let full = crate::infra::db::app_data_path(&app, &rel);
        if !full.exists() {
            crate::log_warn!("image_missing: id={id} file_name={file_name} caller=get_image_src");
        }
        Ok(full.to_string_lossy().into_owned())
    })
    .await
}

/// 更新图像详情字段（文件名、备注、收藏、安全评级）。埋点口径与提示词详情对称。
#[tauri::command]
#[specta::specta]
pub async fn update_image_detail(
    db: State<'_, BkDb>,
    id: String,
    file_name: Option<String>,
    note: Option<String>,
    is_favorite: Option<bool>,
    is_safe: Option<bool>,
) -> Result<Image, AppError> {
    let _timed = crate::commands::timed("update_image_detail");
    crate::log_debug!(
        "update_image_detail 字段 id={id} file_name={} note={} favorite={} safe={}",
        file_name.is_some(),
        note.is_some(),
        is_favorite.is_some(),
        is_safe.is_some()
    );
    db::blocking(&db, move |conn| {
        image_service::update_detail(
            conn,
            &id,
            file_name.as_deref(),
            note.as_deref(),
            is_favorite,
            is_safe,
        )
        .map_err(|e| AppError::Message(e.to_string()))?
        .ok_or_else(|| "图像不存在".into())
    })
    .await
}

/// 同步图像的安全评级到其关联提示词（修改图像安全评级时联动一层，参考 pm 的双向联动）。
#[tauri::command]
#[specta::specta]
pub async fn sync_image_safe_to_prompts(
    db: State<'_, BkDb>,
    image_id: String,
    is_safe: bool,
) -> Result<usize, AppError> {
    db::blocking(&db, move |conn| {
        conn.execute(
            "UPDATE prompts SET is_safe = ?1
             WHERE is_deleted = 0 AND id IN (
               SELECT prompt_id FROM prompt_image_relations WHERE image_id = ?2
             )",
            rusqlite::params![is_safe, image_id],
        )
        .map_err(|e| AppError::Message(e.to_string()))
    })
    .await
}

/// 图像详情解绑提示词：从图像移除一条提示词关联（与提示词侧 remove_image_from_prompt 对称）。
#[tauri::command]
#[specta::specta]
pub async fn remove_prompt_from_image(
    db: State<'_, BkDb>,
    image_id: String,
    prompt_id: String,
) -> Result<(), AppError> {
    db::blocking(&db, move |conn| {
        prompt_service::remove_image(conn, &prompt_id, &image_id)
            .map_err(|e| AppError::Message(e.to_string()))
    })
    .await
}

/// 返回非删除图像到其关联提示词内容的映射：{imageId: [content,...]}，供卡片 row2 显示。
#[tauri::command]
#[specta::specta]
pub async fn get_image_prompts_map(
    db: State<'_, BkDb>,
) -> Result<std::collections::HashMap<String, Vec<String>>, AppError> {
    db::blocking(&db, move |conn| {
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

/// 回收站图像卡片正文：{imageId: 首条关联提示词内容}。
/// 与主页 `get_image_prompts_map` 的差异是纳入已软删的关联提示词
/// （回收站特殊语义：关系还在就显示，哪怕提示词也在提示词回收站里）；按 id 批量取，只回首条。
#[tauri::command]
#[specta::specta]
pub async fn get_trashed_image_first_prompts(
    db: State<'_, BkDb>,
    ids: Vec<String>,
) -> Result<std::collections::HashMap<String, String>, AppError> {
    db::blocking(&db, move |conn| {
        image_service::first_prompts_for_trashed(conn, &ids)
            .map_err(|e| AppError::Message(e.to_string()))
    })
    .await
}

/// 返回单张图像关联的提示词列表（含标题/内容/翻译/备注/标签），供详情页左侧展示。
#[tauri::command]
#[specta::specta]
pub async fn get_image_related_prompts(
    db: State<'_, BkDb>,
    id: String,
) -> Result<Vec<LinkedPrompt>, AppError> {
    db::blocking(&db, move |conn| {
        image_service::list_related_prompts(conn, &id).map_err(|e| AppError::Message(e.to_string()))
    })
    .await
}

/// 为指定图像新建提示词并关联（复用 create_prompt + relate），供图像详情「新建提示词」。
#[tauri::command]
#[specta::specta]
pub async fn create_prompt_for_image(
    db: State<'_, BkDb>,
    content: String,
    image_id: String,
) -> Result<(), AppError> {
    // 两步在同一段短锁内完成：失败要整体回滚（不能留下没关联的提示词）
    db::blocking(&db, move |conn| {
        let prompt = prompt_service::create(conn, &content, None)
            .map_err(|e| AppError::Message(e.to_string()))?;
        image_service::relate_image_to_prompt(conn, &prompt.id, &image_id)
            .map_err(|e| AppError::Message(e.to_string()))?;
        Ok(())
    })
    .await
}

/// 设置页「重建缩略图」：扫描全部图像，补齐丢失的缩略图文件并回写路径，
/// 进度经 thumbnail-rebuild-progress 事件推送。
/// 三阶段：**短锁取清单 → 无锁满核生成 → 短锁批量回写**（生成期间不持锁，见 docs/lessons.md 第 28 节）。
#[tauri::command]
#[specta::specta]
pub async fn rebuild_thumbnails(
    app: tauri::AppHandle,
    db: State<'_, BkDb>,
) -> Result<ThumbnailRebuildSummary, AppError> {
    let data_dir = crate::infra::db::data_dir(&app);
    let thumbs_root = crate::infra::db::thumbnails_dir(&app);
    let targets = db::blocking(&db, |conn| {
        thumbnail_service::all_targets(conn).map_err(AppError::Message)
    })
    .await?;
    let total = targets.len();

    let task_app = app.clone();
    let outcome = crate::infra::task::spawn_blocking("重建缩略图", move || {
        // 满核跑：代价是峰值内存（每线程一张解码位图，详见 docs/导入优化.md）
        let workers = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4);
        Ok(thumbnail_service::build_missing(
            &data_dir,
            &thumbs_root,
            &targets,
            workers,
            |done, total, file_name| {
                let _ = ThumbnailRebuildProgress {
                    current: done,
                    total,
                    file_name: file_name.to_string(),
                }
                .emit(&task_app);
            },
        ))
    })
    .await?;

    let fixed = outcome.fixed;
    db::blocking(&db, move |conn| {
        thumbnail_service::write_paths(conn, &fixed).map_err(AppError::Message)
    })
    .await?;

    Ok(ThumbnailRebuildSummary {
        total,
        success: outcome.success,
        failed: outcome.missing.len(),
    })
}

/// 懒自愈：批量校验指定图像的缩略图文件，缺失且原图存在时按需生成并回写。
/// 正常路径仅 N 次文件存在性检查；同样按「短锁取目标 → 无锁生成 → 短锁回写」推进，
/// 生成期间不持锁（懒自愈是最常见的长时间持锁来源）。
#[tauri::command]
#[specta::specta]
pub async fn ensure_image_thumbnails(
    app: tauri::AppHandle,
    db: State<'_, BkDb>,
    ids: Vec<String>,
) -> Result<ThumbnailEnsureResult, AppError> {
    let data_dir = crate::infra::db::data_dir(&app);
    let thumbs_root = crate::infra::db::thumbnails_dir(&app);
    let (targets, unknown) = db::blocking(&db, move |conn| {
        thumbnail_service::targets_by_ids(conn, &ids).map_err(AppError::Message)
    })
    .await?;

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

    let fixed = outcome.fixed;
    let write = fixed.clone();
    db::blocking(&db, move |conn| {
        thumbnail_service::write_paths(conn, &write).map_err(AppError::Message)
    })
    .await?;

    let mut missing = unknown;
    missing.extend(outcome.missing);
    missing.sort();
    Ok(ThumbnailEnsureResult { fixed, missing })
}

/// 按 id 列表取卡片投影（顺序与入参一致，缺失 / 已删除的跳过）。
/// 供「按 id 渲染卡片」的场景使用，如相似度检索结果。
#[tauri::command]
#[specta::specta]
pub async fn image_cards_by_ids(
    db: State<'_, BkDb>,
    ids: Vec<String>,
) -> Result<Vec<ImageCard>, AppError> {
    db::blocking(&db, move |conn| {
        image_service::cards_by_ids(conn, &ids).map_err(AppError::from)
    })
    .await
}
