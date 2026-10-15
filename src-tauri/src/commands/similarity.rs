//! 图像相似度命令层：索引（全量 / 增量）与检索的编排 + 进度事件。
//! 业务逻辑在 `domain::similarity_service`，向量来源经 `infra::embedding_client` 注入。
//!
//! 三个刻意的设计：
//! - 循环不放进 domain：万级 × ~0.5s 的长任务若整段持有 DB 锁，主页查询会被卡死；
//!   这里按「短锁取清单 → 无锁预处理 / 请求服务 → 短锁写回」的节奏推进。
//! - 进度**既可订阅也可查询**：事件不重放，离开设置页期间推送的增量会丢，
//!   因此把当前快照存在 managed state 里，重进页面用 `similarity_index_progress` 取回。
//! - **embedding 服务是可选能力**（llama.cpp 不常驻）：任务开跑前先探测、连续失败即中止、
//!   随时可取消，进度与结果都带中止原因；服务没起时不该把整表跑一遍，也不该让用户
//!   在没有取消入口的情况下等上小时级（见 docs/lessons.md 第 28 节）。
//!
//! 并发度由调用方给出（设置页可调，默认 2）：图像侧受服务端视觉编码的 CPU 限制，
//! 实测 4 路并发仅约 1.2x（预处理与服务端编码可重叠，收益主要来自这一点），
//! 因此默认不开满；建议不超过服务端 `-np`。

use crate::domain::similarity_service::{
    self, ImageHit, IndexMode, PromptHit, SimilarImageHits, SimilarPromptHits, SimilarityStatus,
};
use crate::infra::db::{self, BkDb};
use crate::infra::embedding_client::{self, EmbeddingServiceInfo};
use crate::infra::error::AppError;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::Instant;
use tauri::{AppHandle, Manager, State};
use tauri_specta::Event;

/// 连续失败多少条即中止索引任务：服务没起时不必把整表跑完（与 embedding 侧断路器互补——
/// 断路器挡住请求，这里结束遍历并给出可读原因）。
const MAX_CONSECUTIVE_FAILURES: usize = 5;
/// 进度事件按 `total / PROGRESS_STEPS` 的步长上报：万级索引原本每条一次 emit，
/// 会把 webview 的消息队列刷爆；快照仍每条更新（重进设置页取回的永远是最新值）。
const PROGRESS_STEPS: usize = 200;

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
    /// 中止原因（用户取消 / 连续失败）；正常跑完为空串
    pub reason: String,
}

/// 索引任务状态：重入保护 + 取消请求 + 当前进度快照。
#[derive(Default)]
pub struct SimilarityState {
    indexing: AtomicBool,
    /// 取消请求：设置页「取消」置位，任务在下一个条目边界收尾（在途请求跑完为止）
    cancel: AtomicBool,
    progress: Mutex<SimilarityIndexProgress>,
}

/// 索引结果统计。
#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct SimilarityIndexSummary {
    /// 本次任务的待处理条数（中止时未必跑完）
    pub total: usize,
    /// **已建成向量**（成功）的条数：中止时小于 `total - failed`（没轮到的条目算「未处理」）
    pub indexed: usize,
    pub failed: usize,
    /// 是否被中止（用户取消 / 维度不一致 / 连续失败）
    pub aborted: bool,
    /// 中止原因；正常跑完为空串
    pub reason: String,
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
                db::blocking(&db, move |conn| similarity_service::vec_of(conn, &id)).await?
            {
                return Ok(v);
            }
            // 服务被断路器挡住（多半是没启动）：立刻给可读错误，
            // 别先查相对路径、再解码预处理成 JPEG，最后白等一次超时
            if let Some(msg) = embedding_client::service_unavailable() {
                return Err(AppError::Message(msg));
            }
            let id = source_id.to_string();
            let rel = db::blocking(&db, move |conn| {
                similarity_service::relative_path_of(conn, &id)
            })
            .await?
            .ok_or_else(|| AppError::Message(format!("图像 {source_id} 不存在")))?;
            let jpeg = similarity_service::prepare_image(&db::data_path(&db::data_dir(app), &rel))?;
            let url = base_url.to_string();
            let vec = crate::infra::task::spawn_blocking("检索", move || {
                embedding_client::make_embedder(&url).embed_image(&jpeg)
            })
            .await?;
            let id = source_id.to_string();
            let stored = vec.clone();
            db::blocking(&db, move |conn| {
                similarity_service::store(conn, &id, &stored)
            })
            .await?;
            Ok(vec)
        }
        MixedSource::Prompt => {
            let id = source_id.to_string();
            if let Some(v) = db::blocking(&db, move |conn| {
                similarity_service::prompt_vec_of(conn, &id)
            })
            .await?
            {
                return Ok(v);
            }
            // 同图像侧：先预检再取内容，免得白跑一次必然失败的请求
            if let Some(msg) = embedding_client::service_unavailable() {
                return Err(AppError::Message(msg));
            }
            let id = source_id.to_string();
            let content = db::blocking(&db, move |conn| {
                similarity_service::prompt_content_of(conn, &id)
            })
            .await?
            .ok_or_else(|| AppError::Message(format!("提示词 {source_id} 不存在")))?;
            let url = base_url.to_string();
            // 与批量索引同口径：向量输入先规范化，排版差异（换行 / 标点旁空白）不改变向量
            let content = similarity_service::canonical_text(&content);
            let vec = crate::infra::task::spawn_blocking("检索", move || {
                embedding_client::make_embedder(&url).embed_text(&content)
            })
            .await?;
            let id = source_id.to_string();
            let stored = vec.clone();
            db::blocking(&db, move |conn| {
                similarity_service::store_prompt(conn, &id, &stored)
            })
            .await?;
            Ok(vec)
        }
    }
}

/// 更新快照并广播进度：只在 `current` 前进时发布（多线程完成顺序不定，避免进度回退）；
/// 事件按 [`PROGRESS_STEPS`] 降频推送，但首条与末条（`running = false`，带中止原因）必推。
fn publish(app: &AppHandle, state: &SimilarityState, next: SimilarityIndexProgress) {
    let mut emit = true;
    if let Ok(mut slot) = state.progress.lock() {
        if next.running && next.current < slot.current {
            return;
        }
        let step = (next.total / PROGRESS_STEPS).max(1);
        emit = !next.running || next.current % step == 0;
        *slot = next.clone();
    }
    if emit {
        let _ = next.emit(app);
    }
}

/// 当前索引进度快照（设置页挂载时查询，事件不重放）。
#[tauri::command]
#[specta::specta]
pub fn similarity_index_progress(state: State<'_, SimilarityState>) -> SimilarityIndexProgress {
    state.progress.lock().map(|p| p.clone()).unwrap_or_default()
}

/// 本地索引状态（不访问服务）：总数 / 已索引 / 各维度的条数分布。
#[tauri::command]
#[specta::specta]
pub async fn similarity_status(db: State<'_, BkDb>) -> Result<SimilarityStatus, AppError> {
    db::blocking(&db, similarity_service::status).await
}

/// 探测 embedding 服务（设置页「测试连通性」）：模型名 / media marker / 维度 / 是否加载视觉塔。
/// 这是用户显式发起的重试，故先复位断路器（冷却不该拦住明确的用户意图）。
#[tauri::command]
#[specta::specta]
pub async fn embedding_service_info(base_url: String) -> Result<EmbeddingServiceInfo, AppError> {
    crate::infra::task::spawn_blocking("探测", move || {
        embedding_client::reset_breaker();
        embedding_client::make_embedder(&base_url).info()
    })
    .await
}

/// 清空全部向量（设置页「清空」，之后可重建）。
#[tauri::command]
#[specta::specta]
pub async fn clear_image_embeddings(db: State<'_, BkDb>) -> Result<usize, AppError> {
    db::blocking(&db, similarity_service::clear_all).await
}

/// 只清空**指定维度**的图像向量（设置页维度分布里的「清空 xxxx」）。
/// 换模型后旧维度的向量无法参与新维度的检索，清掉它再点「增量索引」即可重建，
/// 不必为少数旧向量做整表全量重建。
#[tauri::command]
#[specta::specta]
pub async fn clear_image_embeddings_by_dim(
    db: State<'_, BkDb>,
    dim: usize,
) -> Result<usize, AppError> {
    db::blocking(&db, move |conn| {
        similarity_service::clear_image_dim(conn, dim)
    })
    .await
}

/// 请求取消正在跑的图像索引：任务在下一个条目边界收尾（在途请求跑完为止）。
/// 返回当前是否确实有任务在跑（未运行时无副作用，避免误报「已取消」）。
#[tauri::command]
#[specta::specta]
pub fn cancel_image_index(state: State<'_, SimilarityState>) -> bool {
    state.cancel.store(true, Ordering::SeqCst);
    state.indexing.load(Ordering::SeqCst)
}

/// 建立图像向量索引：`Full` 先清空全部向量再全量，`Incremental` 只补 `vec IS NULL`。
/// `concurrency` 为客户端并发请求数（1~8，建议 ≤ 服务端 `-np`）。
/// 开跑前先探测服务（不可用直接失败，**不清空**已有向量）；连续 [`MAX_CONSECUTIVE_FAILURES`]
/// 条失败即中止；用户可随时 `cancel_image_index`。
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
    state.cancel.store(false, Ordering::SeqCst);
    // 线程内需要 `'static` 的 AppHandle：`state` 借用了 `app`，故克隆一份交给任务
    let task_app = app.clone();
    let workers = concurrency.clamp(1, 8);
    let result = crate::infra::task::spawn_blocking("图像索引", move || {
        // 用户显式点了索引：先复位断路器，再取一次服务信息（连通性预检 + 视觉塔能力 + 实测维度）
        embedding_client::reset_breaker();
        let embedder = embedding_client::make_embedder(&base_url);
        let info = embedder.info()?;
        // 视觉塔没加载时图像向量根本不可用：在**清空已有向量之前**拦下（全量模式下先毁再中止是最坏的顺序）
        if let Some(reason) = info.vision_unsupported_reason() {
            return Err(AppError::Message(reason));
        }
        let data_dir = db::data_dir(&task_app);
        let bk = task_app.state::<BkDb>();
        let task_state = task_app.state::<SimilarityState>();
        let lock = || bk.0.lock().map_err(|e| AppError::Message(e.to_string()));

        if mode == IndexMode::Full {
            let conn = lock()?;
            similarity_service::clear_all(&conn)?;
        }
        // 索引前读一次本表已存维度（Full 已清空 → None），作增量期新向量的一致性比对基准。
        // 只比本表 —— 图像与提示词的维度允许不同（两域各自独立的空间）
        let stored_dim = {
            let conn = lock()?;
            similarity_service::image_stored_dim(&conn)?
        };
        let pending = {
            let conn = lock()?;
            similarity_service::pending(&conn)?
        };
        let total = pending.len();
        // 开跑前就中止的统一收尾：一条都不参与计算（已建 / 失败 均 0，未处理 = total）。
        // 必须发一次完成态 —— 面板「上次索引」读它才显示得出「未处理 N」。
        let abort_before_run = |reason: String| -> Result<SimilarityIndexSummary, AppError> {
            crate::log_warn!("图像索引未开始：{reason}");
            publish(
                &task_app,
                &task_state,
                SimilarityIndexProgress {
                    running: false,
                    current: 0,
                    total,
                    failed: 0,
                    file_name: String::new(),
                    eta_ms: 0,
                    reason: reason.clone(),
                },
            );
            Ok(SimilarityIndexSummary {
                total,
                indexed: 0,
                failed: 0,
                aborted: true,
                reason,
            })
        };
        // 开跑前真探测：量服务**当前**实际返回的向量长度（`/v1/models` 的 `meta.n_embd` 不可信，
        // 本机实测声明 512 而真实 768）。与本表基准不符就一条都不算 —— 等算完再拦（逐条兜底）
        // 等于白跑一整遍，还把没跑到的条目显示成「成功」。
        if let Some(reason) =
            probe_image_dim_abort_reason(embedder.as_ref(), stored_dim, total > 0)?
        {
            return abort_before_run(reason);
        }
        crate::log_info!("图像索引开始 total={total} 并发={workers} mode={mode:?}");
        // 域的差异点收进钩子，主体遍历交给共用的 run_index（图像与提示词走同一条路径）
        let hooks: IndexHooks<similarity_service::PendingImage> = IndexHooks {
            // 路径统一走 db::data_path（relative_path 相对数据目录，勿用 images_dir 再拼）
            embed: Box::new(|item: &similarity_service::PendingImage| {
                similarity_service::prepare_image(&db::data_path(&data_dir, &item.relative_path))
                    .and_then(|jpeg| embedder.embed_image(&jpeg))
            }),
            store: Box::new(|item: &similarity_service::PendingImage, vec: &[f32]| {
                let conn = lock()?;
                similarity_service::store(&conn, &item.id, vec)
            }),
            tag: Box::new(|item: &similarity_service::PendingImage| item.id.clone()),
            label: Box::new(|item: &similarity_service::PendingImage| item.relative_path.clone()),
        };
        let emit = |t: IndexTick| {
            publish(
                &task_app,
                &task_state,
                SimilarityIndexProgress {
                    running: t.running,
                    current: t.current,
                    total: t.total,
                    failed: t.failed,
                    file_name: t.label,
                    eta_ms: t.eta_ms,
                    reason: t.reason,
                },
            );
        };
        let started = Instant::now();
        let outcome = run_index(
            "图像索引",
            &pending,
            stored_dim,
            workers,
            &task_state.cancel,
            &hooks,
            &emit,
        );
        let suffix = if outcome.reason.is_empty() {
            String::new()
        } else {
            format!("原因={}", outcome.reason)
        };
        crate::log_info!(
            "图像索引结束 current={}/{} 失败={} 耗时={}ms {suffix}",
            outcome.current,
            total,
            outcome.failed,
            started.elapsed().as_millis()
        );
        emit(IndexTick {
            running: false,
            current: outcome.current,
            total,
            failed: outcome.failed,
            label: String::new(),
            eta_ms: 0,
            reason: outcome.reason.clone(),
        });
        Ok(SimilarityIndexSummary {
            total,
            indexed: outcome.current - outcome.failed,
            failed: outcome.failed,
            aborted: !outcome.reason.is_empty(),
            reason: outcome.reason,
        })
    })
    .await;
    state.indexing.store(false, Ordering::SeqCst);
    result
}

/// 任务收尾时的中止原因（空串 = 正常跑完）：用户取消 > 维度不一致 > 连续失败。
/// `conflict_dim` 为 0 表示未检出维度不一致；`stored_dim` 仅用于把冲突的旧维度报出来。
/// 图像与提示词的索引状态类型不同，但取消标志都是 `AtomicBool`，故两侧共用本函数（只收取消标志）。
fn halt_reason(
    cancel: &AtomicBool,
    halted: bool,
    conflict_dim: usize,
    stored_dim: Option<usize>,
) -> String {
    if cancel.load(Ordering::SeqCst) {
        "已取消".to_string()
    } else if conflict_dim > 0 {
        similarity_service::dim_conflict_reason(conflict_dim, stored_dim)
    } else if halted {
        format!("连续 {MAX_CONSECUTIVE_FAILURES} 条失败已中止（疑似 embedding 服务不可用）")
    } else {
        String::new()
    }
}

/// 开跑前的维度探测结论 → 应中止的原因（`None` = 可继续）。图像与提示词共用：
/// 探测维度由各自拿（图像真跑一张探测图，提示词直接用 `info.dim` —— `info()` 里已实测过）。
/// 无待处理或没有基准时一律放行，连探测请求都不必发。
fn probe_dim_abort_reason(
    probe_dim: usize,
    stored_dim: Option<usize>,
    has_pending: bool,
) -> Option<String> {
    (has_pending && similarity_service::dim_conflict(stored_dim, probe_dim))
        .then(|| similarity_service::dim_conflict_reason(probe_dim, stored_dim))
}

/// 图像侧：先真跑一张探测图量服务**当前**返回的维度，再交给 [`probe_dim_abort_reason`]。
/// 无待处理或没有基准时跳过，省一次请求（`meta.n_embd` 不可信，只能实测，见 [`similarity_service::probe_jpeg`]）。
fn probe_image_dim_abort_reason(
    embedder: &dyn embedding_client::Embedder,
    stored_dim: Option<usize>,
    has_pending: bool,
) -> Result<Option<String>, AppError> {
    if !has_pending || stored_dim.is_none() {
        return Ok(None);
    }
    let probe_dim = embedder
        .embed_image(&similarity_service::probe_jpeg()?)?
        .len();
    Ok(probe_dim_abort_reason(probe_dim, stored_dim, has_pending))
}

// —————————————————————— 索引遍历的共用骨架 ——————————————————————
// 图像与提示词的索引主体只差「条目怎么预处理 + 怎么写回 + 进度事件类型」，其余（三段式节奏、
// 维度一致性护栏、连续失败中止、取消、ETA、收尾统计）完全同形，故收进 [`run_index`]，
// 差异经 [`IndexHooks`] 注入 —— 两侧各自只留命令层薄壳。

/// 索引遍历的一步进度快照（域无关）：命令层把它映射成各自的事件结构后发布。
#[derive(Clone)]
struct IndexTick {
    running: bool,
    current: usize,
    total: usize,
    failed: usize,
    /// 当前条目标签（图像=相对路径，提示词=标题）
    label: String,
    eta_ms: u64,
    reason: String,
}

/// 索引遍历结束时的统计（命令层据此拼 `SimilarityIndexSummary` 与完成态进度）。
struct IndexOutcome {
    current: usize,
    failed: usize,
    /// 中止原因（空串 = 正常跑完）
    reason: String,
}

/// 索引遍历里「域相关」的钩子：图像 / 提示词各造一份。钩子会跨线程共享，故须 `Sync`；
/// `store` 由主体在短锁内调用，其余都在无锁段调用。
struct IndexHooks<'a, Item> {
    /// 单条处理：预处理 + 请求 embedding 服务 → 向量
    embed: Box<dyn Fn(&Item) -> Result<Vec<f32>, AppError> + Sync + 'a>,
    /// 短锁内写回
    store: Box<dyn Fn(&Item, &[f32]) -> Result<(), AppError> + Sync + 'a>,
    /// 条目主键（失败日志用）
    tag: Box<dyn Fn(&Item) -> String + Sync + 'a>,
    /// 条目标签（进度显示用）
    label: Box<dyn Fn(&Item) -> String + Sync + 'a>,
}

/// 索引任务主体（图像 / 提示词共用）：发布起始进度 → 线程池按「短锁取清单 → 无锁请求服务 →
/// 短锁写回」的节奏遍历 → 收尾统计。维度一致性护栏、连续失败中止、取消、ETA 都在这条共用路径上。
/// 返回的 `reason` 交给调用方拼进 summary 与完成态进度（空串 = 正常跑完）。
fn run_index<Item: Sync>(
    name: &str,
    pending: &[Item],
    stored_dim: Option<usize>,
    workers: usize,
    cancel: &AtomicBool,
    hooks: &IndexHooks<'_, Item>,
    publish: &(dyn Fn(IndexTick) + Sync),
) -> IndexOutcome {
    let total = pending.len();
    let started = Instant::now();
    let cursor = AtomicUsize::new(0);
    let done = AtomicUsize::new(0);
    let failed = AtomicUsize::new(0);
    // 连续失败计数（成功即清零）与中止标志：服务中途挂掉时不必把整表跑完
    let streak = AtomicUsize::new(0);
    let halted = AtomicBool::new(false);
    // 检出维度不一致的新向量维度（0 = 未检出）：终止原因里要报出来，故单独留一格
    let conflict_dim = AtomicUsize::new(0);

    publish(IndexTick {
        running: true,
        current: 0,
        total,
        failed: 0,
        label: String::new(),
        eta_ms: 0,
        reason: String::new(),
    });

    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| loop {
                // 取消 / 已中止：在条目边界退出（在途请求让它跑完）
                if halted.load(Ordering::SeqCst) || cancel.load(Ordering::SeqCst) {
                    break;
                }
                let i = cursor.fetch_add(1, Ordering::SeqCst);
                if i >= total {
                    break;
                }
                let item = &pending[i];
                match (hooks.embed)(item) {
                    Ok(vec) => {
                        // 新向量与本表已存维度不一致（疑似换了 embedding 模型）：直接终止，
                        // 且这条不入库、不计失败、不计已处理 —— 继续写只会把混杂越铺越广
                        if similarity_service::dim_conflict(stored_dim, vec.len()) {
                            if conflict_dim.swap(vec.len(), Ordering::SeqCst) == 0 {
                                crate::log_warn!(
                                    "{name}维度不一致：新向量 {} 维 vs 已存 {:?} 维，已终止",
                                    vec.len(),
                                    stored_dim
                                );
                            }
                            halted.store(true, Ordering::SeqCst);
                            break;
                        }
                        if let Err(e) = (hooks.store)(item, &vec) {
                            failed.fetch_add(1, Ordering::SeqCst);
                            crate::log_warn!("{name}向量写库失败 {}：{e}", (hooks.tag)(item));
                        } else {
                            streak.store(0, Ordering::SeqCst);
                        }
                    }
                    Err(e) => {
                        failed.fetch_add(1, Ordering::SeqCst);
                        crate::log_warn!("{name}失败 {}：{e}", (hooks.tag)(item));
                        if streak.fetch_add(1, Ordering::SeqCst) + 1 >= MAX_CONSECUTIVE_FAILURES
                            && !halted.swap(true, Ordering::SeqCst)
                        {
                            crate::log_warn!(
                                "{name}连续 {MAX_CONSECUTIVE_FAILURES} 条失败，已中止（疑似 embedding 服务不可用）"
                            );
                        }
                    }
                }
                let current = done.fetch_add(1, Ordering::SeqCst) + 1;
                let elapsed = started.elapsed().as_millis() as u64;
                let eta_ms = if current > 0 {
                    elapsed / current as u64 * (total - current) as u64
                } else {
                    0
                };
                publish(IndexTick {
                    running: true,
                    current,
                    total,
                    failed: failed.load(Ordering::SeqCst),
                    label: (hooks.label)(item),
                    eta_ms,
                    reason: String::new(),
                });
            });
        }
    });

    let current = done.load(Ordering::SeqCst);
    let failed = failed.load(Ordering::SeqCst);
    let reason = halt_reason(
        cancel,
        halted.load(Ordering::SeqCst),
        conflict_dim.load(Ordering::SeqCst),
        stored_dim,
    );
    IndexOutcome {
        current,
        failed,
        reason,
    }
}

/// 以任意源检索**图像表**（相似图像）：源可以是图像（同模态）或提示词（跨模态）。
/// 结果页左栏每次只查这一侧，条数 / 阈值由调用方各自传入；恒为全量比较（不过滤 `is_safe`）。
/// 返回 `dim`（本次检索维度）、`dim_mismatch`（本表维度不一致、已被排除的条数）与
/// `pending`（本表未建立向量、不会出现在结果里的条数），供结果页提醒。
#[tauri::command]
#[specta::specta]
pub async fn similar_images(
    app: AppHandle,
    base_url: String,
    source: MixedSource,
    source_id: String,
    limit: usize,
    min_score: f32,
) -> Result<SimilarImageHits, AppError> {
    let target = resolve_target(&app, &base_url, source, &source_id).await?;
    let db = app.state::<BkDb>();
    let limit = limit.clamp(1, 200);
    // 源自身不必出现在结果里；源是提示词时不排除任何图像（传空串即不排除）
    let exclude = if source == MixedSource::Image {
        source_id
    } else {
        String::new()
    };
    // 短锁取候选（顺带数出维度不一致 / 未建立向量的条数）→ 锁外打分：万级向量时打分才是大头，不该占着单连接锁
    let dim_len = target.len();
    let cand = db::blocking(&db, move |conn| {
        similarity_service::image_rank_candidates(conn, dim_len, &exclude)
    })
    .await?;
    Ok(SimilarImageHits {
        hits: similarity_service::score_ranked(&cand.rows, &target, limit, min_score)
            .into_iter()
            .map(|(image_id, score)| ImageHit { image_id, score })
            .collect(),
        dim: dim_len,
        dim_mismatch: cand.dim_mismatch,
        pending: cand.pending,
    })
}

// —————————————————————— 提示词向量索引（文本侧） ——————————————————————
// 与图像侧完全对称：同一套「短锁取清单 → 无锁请求服务 → 短锁写回」节奏，各自独立的
// 重入保护与进度事件（两条索引可分别查看进度）。差别只在：入参是提示词内容原文，
// 无需图像预处理。

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
    /// 中止原因（用户取消 / 连续失败）；正常跑完为空串
    pub reason: String,
}

/// 提示词索引任务状态：重入保护 + 取消请求 + 当前进度快照（与图像侧互不影响）。
#[derive(Default)]
pub struct PromptIndexState {
    indexing: AtomicBool,
    /// 取消请求：设置页「取消」置位，任务在下一个条目边界收尾（在途请求跑完为止）
    cancel: AtomicBool,
    progress: Mutex<PromptIndexProgress>,
}

/// 更新快照并广播：只在 `current` 前进时发布（多线程完成顺序不定，避免进度回退）；
/// 事件按 [`PROGRESS_STEPS`] 降频推送，但首条与末条（`running = false`，带中止原因）必推。
fn publish_prompt(app: &AppHandle, state: &PromptIndexState, next: PromptIndexProgress) {
    let mut emit = true;
    if let Ok(mut slot) = state.progress.lock() {
        if next.running && next.current < slot.current {
            return;
        }
        let step = (next.total / PROGRESS_STEPS).max(1);
        emit = !next.running || next.current % step == 0;
        *slot = next.clone();
    }
    if emit {
        let _ = next.emit(app);
    }
}

/// 当前提示词索引进度快照（设置页挂载时查询，事件不重放）。
#[tauri::command]
#[specta::specta]
pub fn prompt_index_progress(state: State<'_, PromptIndexState>) -> PromptIndexProgress {
    state.progress.lock().map(|p| p.clone()).unwrap_or_default()
}

/// 提示词索引状态（不访问服务）：总数 / 已索引 / 各维度的条数分布。
#[tauri::command]
#[specta::specta]
pub async fn prompt_embedding_status(db: State<'_, BkDb>) -> Result<SimilarityStatus, AppError> {
    db::blocking(&db, similarity_service::prompt_status).await
}

/// 清空全部提示词向量（设置页「清空」，之后可重建）。
#[tauri::command]
#[specta::specta]
pub async fn clear_prompt_embeddings(db: State<'_, BkDb>) -> Result<usize, AppError> {
    db::blocking(&db, similarity_service::clear_prompts).await
}

/// 只清空**指定维度**的提示词向量（设置页维度分布里的「清空 xxxx」）。
/// 换模型后旧维度的向量无法参与新维度的检索，清掉它再点「增量索引」即可重建，
/// 不必为少数旧向量做整表全量重建（口径与图像侧 [`clear_image_embeddings_by_dim`] 对称）。
#[tauri::command]
#[specta::specta]
pub async fn clear_prompt_embeddings_by_dim(
    db: State<'_, BkDb>,
    dim: usize,
) -> Result<usize, AppError> {
    db::blocking(&db, move |conn| {
        similarity_service::clear_prompt_dim(conn, dim)
    })
    .await
}

/// 请求取消正在跑的提示词索引（与图像侧对称）：下一个条目边界收尾；
/// 返回当前是否确实有任务在跑。
#[tauri::command]
#[specta::specta]
pub fn cancel_prompt_index(state: State<'_, PromptIndexState>) -> bool {
    state.cancel.store(true, Ordering::SeqCst);
    state.indexing.load(Ordering::SeqCst)
}

/// 建立提示词向量索引：只对 `prompts.content` 计算向量（不含标题 / 翻译 / 备注）。
/// `Full` 先清空全部向量再全量，`Incremental` 只补 `vec IS NULL`（含内容改过被置空的）。
/// 文本请求无视觉编码，比图像侧轻得多，可适当开大并发（仍建议 ≤ 服务端 `-np`）。
/// 服务预检 / 连续失败中止 / 取消的口径与图像侧完全一致。
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
    state.cancel.store(false, Ordering::SeqCst);
    let task_app = app.clone();
    let workers = concurrency.clamp(1, 8);
    let result = crate::infra::task::spawn_blocking("提示词索引", move || {
        // 用户显式点了索引：先复位断路器，再取一次服务信息（与图像侧对称；
        // 提示词侧只看实测 dim，不看 vision —— 文本不经过视觉塔）
        embedding_client::reset_breaker();
        let embedder = embedding_client::make_embedder(&base_url);
        let info = embedder.info()?;
        let bk = task_app.state::<BkDb>();
        let task_state = task_app.state::<PromptIndexState>();
        let lock = || bk.0.lock().map_err(|e| AppError::Message(e.to_string()));

        if mode == IndexMode::Full {
            let conn = lock()?;
            similarity_service::clear_prompts(&conn)?;
        }
        // 索引前读一次本表已存维度（Full 已清空 → None），作增量期新向量的一致性比对基准。
        // 只比本表 —— 提示词与图像的维度允许不同（两域各自独立的空间）
        let stored_dim = {
            let conn = lock()?;
            similarity_service::prompt_stored_dim(&conn)?
        };
        let pending = {
            let conn = lock()?;
            similarity_service::pending_prompts(&conn)?
        };
        let total = pending.len();
        // 开跑前就中止的统一收尾（口径与图像侧一致）：一条都不参与计算，未处理 = total
        let abort_before_run = |reason: String| -> Result<SimilarityIndexSummary, AppError> {
            crate::log_warn!("提示词索引未开始：{reason}");
            publish_prompt(
                &task_app,
                &task_state,
                PromptIndexProgress {
                    running: false,
                    current: 0,
                    total,
                    failed: 0,
                    file_name: String::new(),
                    eta_ms: 0,
                    reason: reason.clone(),
                },
            );
            Ok(SimilarityIndexSummary {
                total,
                indexed: 0,
                failed: 0,
                aborted: true,
                reason,
            })
        };
        // 开跑前真探测（与图像侧对称）：`info()` 里的 `dim` 就是刚跑一次短文本量出来的长度，
        // 故文本侧不必再多一次请求。与本表基准不符就一条都不算。
        if let Some(reason) = probe_dim_abort_reason(info.dim, stored_dim, total > 0) {
            return abort_before_run(reason);
        }
        crate::log_info!("提示词索引开始 total={total} 并发={workers} mode={mode:?}");
        // 域的差异点收进钩子，主体遍历交给共用的 run_index（与图像侧对称）
        let hooks: IndexHooks<similarity_service::PendingPrompt> = IndexHooks {
            // 规范化后送模型（与检索现场补算同口径）；超长（服务端上下文不足）等原因的失败计入 failed
            embed: Box::new(|item: &similarity_service::PendingPrompt| {
                embedder.embed_text(&similarity_service::canonical_text(&item.content))
            }),
            store: Box::new(|item: &similarity_service::PendingPrompt, vec: &[f32]| {
                let conn = lock()?;
                similarity_service::store_prompt(&conn, &item.id, vec)
            }),
            tag: Box::new(|item: &similarity_service::PendingPrompt| item.id.clone()),
            label: Box::new(|item: &similarity_service::PendingPrompt| item.title.clone()),
        };
        let emit = |t: IndexTick| {
            publish_prompt(
                &task_app,
                &task_state,
                PromptIndexProgress {
                    running: t.running,
                    current: t.current,
                    total: t.total,
                    failed: t.failed,
                    file_name: t.label,
                    eta_ms: t.eta_ms,
                    reason: t.reason,
                },
            );
        };
        let started = Instant::now();
        let outcome = run_index(
            "提示词索引",
            &pending,
            stored_dim,
            workers,
            &task_state.cancel,
            &hooks,
            &emit,
        );
        let suffix = if outcome.reason.is_empty() {
            String::new()
        } else {
            format!("原因={}", outcome.reason)
        };
        crate::log_info!(
            "提示词索引结束 current={}/{} 失败={} 耗时={}ms {suffix}",
            outcome.current,
            total,
            outcome.failed,
            started.elapsed().as_millis()
        );
        emit(IndexTick {
            running: false,
            current: outcome.current,
            total,
            failed: outcome.failed,
            label: String::new(),
            eta_ms: 0,
            reason: outcome.reason.clone(),
        });
        Ok(SimilarityIndexSummary {
            total,
            indexed: outcome.current - outcome.failed,
            failed: outcome.failed,
            aborted: !outcome.reason.is_empty(),
            reason: outcome.reason,
        })
    })
    .await;
    state.indexing.store(false, Ordering::SeqCst);
    result
}

/// 以任意源检索**提示词表**（相似提示词）：源可以是提示词（同模态）或图像（跨模态）。
/// 结果页右栏每次只查这一侧，条数 / 阈值由调用方各自传入；提示词不过滤 `is_safe`。
/// 返回 `dim`（本次检索维度）、`dim_mismatch`（本表维度不一致、已被排除的条数）与
/// `pending`（本表未建立向量、不会出现在结果里的条数），供结果页提醒。
#[tauri::command]
#[specta::specta]
pub async fn similar_prompts(
    app: AppHandle,
    base_url: String,
    source: MixedSource,
    source_id: String,
    limit: usize,
    min_score: f32,
) -> Result<SimilarPromptHits, AppError> {
    let target = resolve_target(&app, &base_url, source, &source_id).await?;
    let db = app.state::<BkDb>();
    let limit = limit.clamp(1, 200);
    // 源自身不必出现在结果里；源是图像时不排除任何提示词（传空串即不排除）
    let exclude = if source == MixedSource::Prompt {
        source_id
    } else {
        String::new()
    };
    // 短锁取候选（顺带数出维度不一致 / 未建立向量的条数）→ 锁外打分（与图像侧对称）
    let dim_len = target.len();
    let cand = db::blocking(&db, move |conn| {
        similarity_service::prompt_rank_candidates(conn, dim_len, &exclude)
    })
    .await?;
    Ok(SimilarPromptHits {
        hits: similarity_service::score_ranked(&cand.rows, &target, limit, min_score)
            .into_iter()
            .map(|(prompt_id, score)| PromptHit { prompt_id, score })
            .collect(),
        dim: dim_len,
        dim_mismatch: cand.dim_mismatch,
        pending: cand.pending,
    })
}

#[cfg(test)]
#[path = "similarity.test.rs"]
mod tests;
