//! 图像相似度命令层：索引（全量 / 增量）与检索的编排 + 进度事件。
//! 业务逻辑在 `domain::similarity_service`，向量来源经 `infra::embedding_client` 注入。
//!
//! 循环不放进 domain 的原因：万级 × ~0.5s 的长任务若整段持有 DB 锁，主页查询会被卡死；
//! 这里按「短锁取清单 → 无锁预处理 / 请求服务 → 短锁写回」的节奏推进。

use crate::commands::db_blocking;
use crate::domain::similarity_service::{self, IndexMode, SimilarHit, SimilarityStatus};
use crate::infra::db::{self, BkDb};
use crate::infra::embedding_client::{self, EmbeddingServiceInfo};
use crate::infra::error::AppError;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{AppHandle, Manager, State};
use tauri_specta::Event;

/// 索引进度（设置页进度条）。
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, tauri_specta::Event)]
#[tauri_specta(event_name = "similarity-index-progress")]
pub struct SimilarityIndexProgress {
    pub current: usize,
    pub total: usize,
    pub file_name: String,
    pub failed: usize,
}

/// 索引任务的重入保护：并发跑两次会把同一张图算两遍、进度互相覆盖。
#[derive(Default)]
pub struct SimilarityState {
    indexing: AtomicBool,
}

/// 索引结果统计。
#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct SimilarityIndexSummary {
    pub total: usize,
    pub indexed: usize,
    pub failed: usize,
}

/// 本地索引状态（不访问服务）：总数 / 已索引 / 维度 / 需重建数。
#[tauri::command]
#[specta::specta]
pub async fn similarity_status(db: State<'_, BkDb>) -> Result<SimilarityStatus, AppError> {
    db_blocking(&db, similarity_service::status).await
}

/// 探测 embedding 服务（设置页「测试连通性」）：模型名 / media marker / 维度 / 是否加载视觉塔。
#[tauri::command]
#[specta::specta]
pub async fn embedding_service_info(base_url: String) -> Result<EmbeddingServiceInfo, AppError> {
    tauri::async_runtime::spawn_blocking(move || embedding_client::make_embedder(&base_url).info())
        .await
        .map_err(|e| AppError::Message(format!("探测任务执行失败: {e}")))?
}

/// 清空全部向量（设置页「清空」，之后可重建）。
#[tauri::command]
#[specta::specta]
pub async fn clear_image_embeddings(db: State<'_, BkDb>) -> Result<usize, AppError> {
    db_blocking(&db, similarity_service::clear_all).await
}

/// 建立图像向量索引：`Full` 先清空全部向量再全量，`Incremental` 只补 `vec IS NULL`。
/// 逐张「预处理 + 请求 embedding 服务」（不持锁）后短锁写回，进度经事件推送。
#[tauri::command]
#[specta::specta]
pub async fn index_image_embeddings(
    app: AppHandle,
    base_url: String,
    mode: IndexMode,
) -> Result<SimilarityIndexSummary, AppError> {
    let state = app.state::<SimilarityState>();
    if state.indexing.swap(true, Ordering::SeqCst) {
        return Err(AppError::Message("索引任务已在运行".into()));
    }
    // 线程内需要 `'static` 的 AppHandle：`state` 借用了 `app`，故克隆一份交给任务
    let task_app = app.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let embedder = embedding_client::make_embedder(&base_url);
        let data_dir = db::data_dir(&task_app);
        let bk = task_app.state::<BkDb>();
        let lock = || bk.0.lock().map_err(|e| AppError::Message(e.to_string()));

        if mode == IndexMode::Full {
            let conn = lock()?;
            similarity_service::clear_all(&conn)?;
        }
        let pending = {
            let conn = lock()?;
            similarity_service::pending(&conn)?
        };
        let total = pending.len();
        let mut indexed = 0usize;
        let mut failed = 0usize;
        for (i, item) in pending.iter().enumerate() {
            // 路径统一走 db::data_path（relative_path 相对数据目录，勿用 images_dir 再拼）
            let outcome =
                similarity_service::prepare_image(&db::data_path(&data_dir, &item.relative_path))
                    .and_then(|jpeg| embedder.embed_image(&jpeg));
            match outcome {
                Ok(vec) => {
                    let conn = lock()?;
                    similarity_service::store(&conn, &item.id, &vec)?;
                    indexed += 1;
                }
                Err(e) => {
                    failed += 1;
                    log::warn!("相似度索引失败 {}：{e}", item.id);
                }
            }
            let _ = SimilarityIndexProgress {
                current: i + 1,
                total,
                file_name: item.relative_path.clone(),
                failed,
            }
            .emit(&task_app);
        }
        Ok(SimilarityIndexSummary {
            total,
            indexed,
            failed,
        })
    })
    .await
    .map_err(|e| AppError::Message(format!("索引任务执行失败: {e}")));
    state.indexing.store(false, Ordering::SeqCst);
    result?
}

/// 以某张图像为查询检索相似图像；目标图未建索引时现场补算一次（约 0.5s）再查。
/// `safe_only` 与主页「安全模式」口径一致；`limit` 上限 200（防止前端误传大值）。
#[tauri::command]
#[specta::specta]
pub async fn similar_images(
    app: AppHandle,
    base_url: String,
    image_id: String,
    limit: usize,
    min_score: f32,
    safe_only: bool,
) -> Result<Vec<SimilarHit>, AppError> {
    tauri::async_runtime::spawn_blocking(move || {
        let data_dir = db::data_dir(&app);
        let bk = app.state::<BkDb>();
        let lock = || bk.0.lock().map_err(|e| AppError::Message(e.to_string()));

        let existing = {
            let conn = lock()?;
            similarity_service::vec_of(&conn, &image_id)?
        };
        let target = match existing {
            Some(v) => v,
            None => {
                let rel = {
                    let conn = lock()?;
                    similarity_service::relative_path_of(&conn, &image_id)?
                        .ok_or_else(|| AppError::Message(format!("图像 {image_id} 不存在")))?
                };
                let jpeg = similarity_service::prepare_image(&db::data_path(&data_dir, &rel))?;
                let vec = embedding_client::make_embedder(&base_url).embed_image(&jpeg)?;
                let conn = lock()?;
                similarity_service::store(&conn, &image_id, &vec)?;
                vec
            }
        };
        let conn = lock()?;
        similarity_service::rank(
            &conn,
            &target,
            &image_id,
            limit.clamp(1, 200),
            min_score,
            safe_only,
        )
    })
    .await
    .map_err(|e| AppError::Message(format!("检索任务执行失败: {e}")))?
}
