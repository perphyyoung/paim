//! 图像相似度命令层：索引（全量 / 增量）与检索的编排 + 进度事件。
//! 业务逻辑在 `domain::similarity_service`，向量来源经 `infra::embedding_client` 注入。
//!
//! 两个刻意的设计：
//! - 循环不放进 domain：万级 × ~0.5s 的长任务若整段持有 DB 锁，主页查询会被卡死；
//!   这里按「短锁取清单 → 无锁预处理 / 请求服务 → 短锁写回」的节奏推进。
//! - 进度**既可订阅也可查询**：事件不重放，离开设置页期间推送的增量会丢，
//!   因此把当前快照存在 managed state 里，重进页面用 `similarity_index_progress` 取回。
//!
//! 并发度由调用方给出（设置页可调，默认 2）：图像侧受服务端视觉编码的 CPU 限制，
//! 实测 4 路并发仅约 1.2x（预处理与服务端编码可重叠，收益主要来自这一点），
//! 因此默认不开满；建议不超过服务端 `-np`。

use crate::commands::db_blocking;
use crate::domain::similarity_service::{self, ImageHit, IndexMode, PromptHit, SimilarityStatus};
use crate::infra::db::{self, BkDb};
use crate::infra::embedding_client::{self, EmbeddingServiceInfo};
use crate::infra::error::AppError;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::Instant;
use tauri::{AppHandle, Manager, State};
use tauri_specta::Event;

/// 索引进度：事件推送与命令查询共用同一结构。
/// `running = false` 时其余字段是上一轮的终值（用于「上次索引」摘要）。
#[derive(Debug, Clone, Default, Serialize, Deserialize, specta::Type, tauri_specta::Event)]
#[tauri_specta(event_name = "similarity-index-progress")]
pub struct SimilarityIndexProgress {
    /// 是否有索引任务在跑
    pub running: bool,
    pub current: usize,
    pub total: usize,
    pub failed: usize,
    pub file_name: String,
    /// 预估剩余毫秒（尚无足够样本时为 0）
    pub eta_ms: u64,
}

/// 索引任务状态：重入保护 + 当前进度快照。
#[derive(Default)]
pub struct SimilarityState {
    indexing: AtomicBool,
    progress: Mutex<SimilarityIndexProgress>,
}

/// 索引结果统计。
#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct SimilarityIndexSummary {
    pub total: usize,
    pub indexed: usize,
    pub failed: usize,
}

/// 结果页的查询源：图像（图像↔图像同模态、图像↔提示词跨模态）或提示词（反之亦然）。
/// 两条检索线共用同一个查询向量 —— 该 embedding 模型是联合空间，跨模态可直接比余弦。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub enum MixedSource {
    Image,
    Prompt,
}

/// 取查询向量：库里已有直接用；没有则现场算一次并写回（下次不必再算）。
/// 图像走「预处理 JPEG（含 webp 转码）→ 视觉编码」，提示词走文本编码；两者产出在同一向量空间。
async fn resolve_target(
    app: &AppHandle,
    base_url: &str,
    source: MixedSource,
    source_id: &str,
) -> Result<Vec<f32>, AppError> {
    let db = app.state::<BkDb>();
    match source {
        MixedSource::Image => {
            let id = source_id.to_string();
            if let Some(v) =
                db_blocking(&db, move |conn| similarity_service::vec_of(conn, &id)).await?
            {
                return Ok(v);
            }
            let id = source_id.to_string();
            let rel = db_blocking(&db, move |conn| {
                similarity_service::relative_path_of(conn, &id)
            })
            .await?
            .ok_or_else(|| AppError::Message(format!("图像 {source_id} 不存在")))?;
            let jpeg = similarity_service::prepare_image(&db::data_path(&db::data_dir(app), &rel))?;
            let url = base_url.to_string();
            let vec = tauri::async_runtime::spawn_blocking(move || {
                embedding_client::make_embedder(&url).embed_image(&jpeg)
            })
            .await
            .map_err(|e| AppError::Message(format!("检索任务执行失败: {e}")))??;
            let id = source_id.to_string();
            let stored = vec.clone();
            db_blocking(&db, move |conn| {
                similarity_service::store(conn, &id, &stored)
            })
            .await?;
            Ok(vec)
        }
        MixedSource::Prompt => {
            let id = source_id.to_string();
            if let Some(v) = db_blocking(&db, move |conn| {
                similarity_service::prompt_vec_of(conn, &id)
            })
            .await?
            {
                return Ok(v);
            }
            let id = source_id.to_string();
            let content = db_blocking(&db, move |conn| {
                similarity_service::prompt_content_of(conn, &id)
            })
            .await?
            .ok_or_else(|| AppError::Message(format!("提示词 {source_id} 不存在")))?;
            let url = base_url.to_string();
            let vec = tauri::async_runtime::spawn_blocking(move || {
                embedding_client::make_embedder(&url).embed_text(&content)
            })
            .await
            .map_err(|e| AppError::Message(format!("检索任务执行失败: {e}")))??;
            let id = source_id.to_string();
            let stored = vec.clone();
            db_blocking(&db, move |conn| {
                similarity_service::store_prompt(conn, &id, &stored)
            })
            .await?;
            Ok(vec)
        }
    }
}

/// 更新快照并广播进度：只在 `current` 前进时发布（多线程完成顺序不定，避免进度回退）。
fn publish(app: &AppHandle, state: &SimilarityState, next: SimilarityIndexProgress) {
    if let Ok(mut slot) = state.progress.lock() {
        if next.running && next.current < slot.current {
            return;
        }
        *slot = next.clone();
    }
    let _ = next.emit(app);
}

/// 当前索引进度快照（设置页挂载时查询，事件不重放）。
#[tauri::command]
#[specta::specta]
pub fn similarity_index_progress(state: State<'_, SimilarityState>) -> SimilarityIndexProgress {
    state.progress.lock().map(|p| p.clone()).unwrap_or_default()
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
/// `concurrency` 为客户端并发请求数（1~8，建议 ≤ 服务端 `-np`）。
#[tauri::command]
#[specta::specta]
pub async fn index_image_embeddings(
    app: AppHandle,
    base_url: String,
    mode: IndexMode,
    concurrency: usize,
) -> Result<SimilarityIndexSummary, AppError> {
    let state = app.state::<SimilarityState>();
    if state.indexing.swap(true, Ordering::SeqCst) {
        return Err(AppError::Message("索引任务已在运行".into()));
    }
    // 线程内需要 `'static` 的 AppHandle：`state` 借用了 `app`，故克隆一份交给任务
    let task_app = app.clone();
    let workers = concurrency.clamp(1, 8);
    let result = tauri::async_runtime::spawn_blocking(move || {
        let embedder = embedding_client::make_embedder(&base_url);
        let data_dir = db::data_dir(&task_app);
        let bk = task_app.state::<BkDb>();
        let task_state = task_app.state::<SimilarityState>();
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
        let started = Instant::now();
        let cursor = AtomicUsize::new(0);
        let done = AtomicUsize::new(0);
        let failed = AtomicUsize::new(0);

        publish(
            &task_app,
            &task_state,
            SimilarityIndexProgress {
                running: true,
                current: 0,
                total,
                failed: 0,
                file_name: String::new(),
                eta_ms: 0,
            },
        );

        std::thread::scope(|scope| {
            for _ in 0..workers {
                scope.spawn(|| loop {
                    let i = cursor.fetch_add(1, Ordering::SeqCst);
                    if i >= total {
                        break;
                    }
                    let item = &pending[i];
                    // 路径统一走 db::data_path（relative_path 相对数据目录，勿用 images_dir 再拼）
                    let outcome = similarity_service::prepare_image(&db::data_path(
                        &data_dir,
                        &item.relative_path,
                    ))
                    .and_then(|jpeg| embedder.embed_image(&jpeg));
                    match outcome {
                        Ok(vec) => match lock() {
                            Ok(conn) => {
                                if let Err(e) = similarity_service::store(&conn, &item.id, &vec) {
                                    failed.fetch_add(1, Ordering::SeqCst);
                                    log::warn!("相似度向量写库失败 {}：{e}", item.id);
                                }
                            }
                            Err(e) => {
                                failed.fetch_add(1, Ordering::SeqCst);
                                log::warn!("相似度向量写库失败 {}：{e}", item.id);
                            }
                        },
                        Err(e) => {
                            failed.fetch_add(1, Ordering::SeqCst);
                            log::warn!("相似度索引失败 {}：{e}", item.id);
                        }
                    }
                    let current = done.fetch_add(1, Ordering::SeqCst) + 1;
                    let elapsed = started.elapsed().as_millis() as u64;
                    let eta_ms = if current > 0 {
                        elapsed / current as u64 * (total - current) as u64
                    } else {
                        0
                    };
                    publish(
                        &task_app,
                        &task_state,
                        SimilarityIndexProgress {
                            running: true,
                            current,
                            total,
                            failed: failed.load(Ordering::SeqCst),
                            file_name: item.relative_path.clone(),
                            eta_ms,
                        },
                    );
                });
            }
        });

        let failed = failed.load(Ordering::SeqCst);
        publish(
            &task_app,
            &task_state,
            SimilarityIndexProgress {
                running: false,
                current: total,
                total,
                failed,
                file_name: String::new(),
                eta_ms: 0,
            },
        );
        Ok(SimilarityIndexSummary {
            total,
            indexed: total - failed,
            failed,
        })
    })
    .await
    .map_err(|e| AppError::Message(format!("索引任务执行失败: {e}")));
    state.indexing.store(false, Ordering::SeqCst);
    result?
}

/// 以任意源检索**图像表**（相似图像）：源可以是图像（同模态）或提示词（跨模态）。
/// 结果页左栏每次只查这一侧，条数 / 阈值由调用方各自传入；`safe_only` 与主页「安全模式」口径一致。
#[tauri::command]
#[specta::specta]
pub async fn similar_images(
    app: AppHandle,
    base_url: String,
    source: MixedSource,
    source_id: String,
    limit: usize,
    min_score: f32,
    safe_only: bool,
) -> Result<Vec<ImageHit>, AppError> {
    let target = resolve_target(&app, &base_url, source, &source_id).await?;
    let db = app.state::<BkDb>();
    let limit = limit.clamp(1, 200);
    // 源自身不必出现在结果里；源是提示词时不排除任何图像（传空串即不排除）
    let exclude = if source == MixedSource::Image {
        source_id
    } else {
        String::new()
    };
    db_blocking(&db, move |conn| {
        similarity_service::rank(conn, &target, &exclude, limit, min_score, safe_only)
    })
    .await
}

// —————————————————————— 提示词向量索引（文本侧） ——————————————————————
// 与图像侧完全对称：同一套「短锁取清单 → 无锁请求服务 → 短锁写回」节奏，各自独立的
// 重入保护与进度事件（两条索引可分别查看进度）。差别只在：入参是提示词内容原文，
// 无需图像预处理，且没有「安全模式」口径。

/// 提示词索引进度：字段与图像侧同形（`file_name` 语义为「当前项标签」，此处取提示词标题），
/// 便于前端复用同一个索引面板组件。
#[derive(Debug, Clone, Default, Serialize, Deserialize, specta::Type, tauri_specta::Event)]
#[tauri_specta(event_name = "prompt-index-progress")]
pub struct PromptIndexProgress {
    /// 是否有索引任务在跑
    pub running: bool,
    pub current: usize,
    pub total: usize,
    pub failed: usize,
    /// 当前项标签（提示词标题）
    pub file_name: String,
    /// 预估剩余毫秒（尚无足够样本时为 0）
    pub eta_ms: u64,
}

/// 提示词索引任务状态：重入保护 + 当前进度快照（与图像侧互不影响）。
#[derive(Default)]
pub struct PromptIndexState {
    indexing: AtomicBool,
    progress: Mutex<PromptIndexProgress>,
}

/// 更新快照并广播：只在 `current` 前进时发布（多线程完成顺序不定，避免进度回退）。
fn publish_prompt(app: &AppHandle, state: &PromptIndexState, next: PromptIndexProgress) {
    if let Ok(mut slot) = state.progress.lock() {
        if next.running && next.current < slot.current {
            return;
        }
        *slot = next.clone();
    }
    let _ = next.emit(app);
}

/// 当前提示词索引进度快照（设置页挂载时查询，事件不重放）。
#[tauri::command]
#[specta::specta]
pub fn prompt_index_progress(state: State<'_, PromptIndexState>) -> PromptIndexProgress {
    state.progress.lock().map(|p| p.clone()).unwrap_or_default()
}

/// 提示词索引状态（不访问服务）：总数 / 已索引 / 维度 / 需重建数。
#[tauri::command]
#[specta::specta]
pub async fn prompt_embedding_status(db: State<'_, BkDb>) -> Result<SimilarityStatus, AppError> {
    db_blocking(&db, similarity_service::prompt_status).await
}

/// 清空全部提示词向量（设置页「清空」，之后可重建）。
#[tauri::command]
#[specta::specta]
pub async fn clear_prompt_embeddings(db: State<'_, BkDb>) -> Result<usize, AppError> {
    db_blocking(&db, similarity_service::clear_prompts).await
}

/// 建立提示词向量索引：只对 `prompts.content` 计算向量（不含标题 / 翻译 / 备注）。
/// `Full` 先清空全部向量再全量，`Incremental` 只补 `vec IS NULL`（含内容改过被置空的）。
/// 文本请求无视觉编码，比图像侧轻得多，可适当开大并发（仍建议 ≤ 服务端 `-np`）。
#[tauri::command]
#[specta::specta]
pub async fn index_prompt_embeddings(
    app: AppHandle,
    base_url: String,
    mode: IndexMode,
    concurrency: usize,
) -> Result<SimilarityIndexSummary, AppError> {
    let state = app.state::<PromptIndexState>();
    if state.indexing.swap(true, Ordering::SeqCst) {
        return Err(AppError::Message("提示词索引任务已在运行".into()));
    }
    let task_app = app.clone();
    let workers = concurrency.clamp(1, 8);
    let result = tauri::async_runtime::spawn_blocking(move || {
        let embedder = embedding_client::make_embedder(&base_url);
        let bk = task_app.state::<BkDb>();
        let task_state = task_app.state::<PromptIndexState>();
        let lock = || bk.0.lock().map_err(|e| AppError::Message(e.to_string()));

        if mode == IndexMode::Full {
            let conn = lock()?;
            similarity_service::clear_prompts(&conn)?;
        }
        let pending = {
            let conn = lock()?;
            similarity_service::pending_prompts(&conn)?
        };
        let total = pending.len();
        let started = Instant::now();
        let cursor = AtomicUsize::new(0);
        let done = AtomicUsize::new(0);
        let failed = AtomicUsize::new(0);

        publish_prompt(
            &task_app,
            &task_state,
            PromptIndexProgress {
                running: true,
                current: 0,
                total,
                failed: 0,
                file_name: String::new(),
                eta_ms: 0,
            },
        );

        std::thread::scope(|scope| {
            for _ in 0..workers {
                scope.spawn(|| loop {
                    let i = cursor.fetch_add(1, Ordering::SeqCst);
                    if i >= total {
                        break;
                    }
                    let item = &pending[i];
                    // 只送内容原文；超长（服务端上下文不足）等原因导致的失败计入 failed 并记日志
                    let outcome = embedder.embed_text(&item.content);
                    match outcome {
                        Ok(vec) => match lock() {
                            Ok(conn) => {
                                if let Err(e) =
                                    similarity_service::store_prompt(&conn, &item.id, &vec)
                                {
                                    failed.fetch_add(1, Ordering::SeqCst);
                                    log::warn!("提示词向量写库失败 {}：{e}", item.id);
                                }
                            }
                            Err(e) => {
                                failed.fetch_add(1, Ordering::SeqCst);
                                log::warn!("提示词向量写库失败 {}：{e}", item.id);
                            }
                        },
                        Err(e) => {
                            failed.fetch_add(1, Ordering::SeqCst);
                            log::warn!("提示词索引失败 {}：{e}", item.id);
                        }
                    }
                    let current = done.fetch_add(1, Ordering::SeqCst) + 1;
                    let elapsed = started.elapsed().as_millis() as u64;
                    let eta_ms = if current > 0 {
                        elapsed / current as u64 * (total - current) as u64
                    } else {
                        0
                    };
                    publish_prompt(
                        &task_app,
                        &task_state,
                        PromptIndexProgress {
                            running: true,
                            current,
                            total,
                            failed: failed.load(Ordering::SeqCst),
                            file_name: item.title.clone(),
                            eta_ms,
                        },
                    );
                });
            }
        });

        let failed = failed.load(Ordering::SeqCst);
        publish_prompt(
            &task_app,
            &task_state,
            PromptIndexProgress {
                running: false,
                current: total,
                total,
                failed,
                file_name: String::new(),
                eta_ms: 0,
            },
        );
        Ok(SimilarityIndexSummary {
            total,
            indexed: total - failed,
            failed,
        })
    })
    .await
    .map_err(|e| AppError::Message(format!("索引任务执行失败: {e}")));
    state.indexing.store(false, Ordering::SeqCst);
    result?
}

/// 以任意源检索**提示词表**（相似提示词）：源可以是提示词（同模态）或图像（跨模态）。
/// 结果页右栏每次只查这一侧，条数 / 阈值由调用方各自传入；提示词不过滤 `is_safe`。
#[tauri::command]
#[specta::specta]
pub async fn similar_prompts(
    app: AppHandle,
    base_url: String,
    source: MixedSource,
    source_id: String,
    limit: usize,
    min_score: f32,
) -> Result<Vec<PromptHit>, AppError> {
    let target = resolve_target(&app, &base_url, source, &source_id).await?;
    let db = app.state::<BkDb>();
    let limit = limit.clamp(1, 200);
    // 源自身不必出现在结果里；源是图像时不排除任何提示词（传空串即不排除）
    let exclude = if source == MixedSource::Prompt {
        source_id
    } else {
        String::new()
    };
    db_blocking(&db, move |conn| {
        similarity_service::rank_prompts(conn, &target, &exclude, limit, min_score)
    })
    .await
}
