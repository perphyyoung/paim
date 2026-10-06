//! 本地 embedding 服务（llama.cpp `llama-server`）客户端：文本 / 图像向量。
//!
//! 设计要点（均为实测约束，依据见 scripts/readme.md 与 todo.md）：
//! - 只访问 127.0.0.1 明文 HTTP：ureq 关闭 TLS 特性，不引入证书栈；
//! - 图像走非 OAI 的 `/embedding`（唯一支持多模态的端点），其 `prompt_string` 必须带
//!   media marker —— 该 marker 由 `/props` 返回且**每个服务进程随机**，因此每次请求现取；
//!   若沿用旧 marker，服务端会静默丢图（向量与画面无关），这是最坏的失败形态；
//! - 文本也走 `/embedding`（`{"content": "文本"}`），与图像处于同一向量空间（二期文搜图用）；
//! - 服务端 `--pooling last` 时返回单条向量、未开时返回逐 token 向量：两种都取**最后一条**
//!   （Qwen3 系以最后一个 token 为句向量），再统一 L2 归一化，检索端只做点积；
//! - 传输层失败重试 1 次（服务端单进程易受抖动影响），HTTP 4xx/5xx 不重试，直接给可读错误；
//! - **失败要「快且可读」**：llama.cpp 不常驻，服务没起时旧口径（总超时 180s × 重试 + 固定
//!   500ms 退避）会让索引把整表空转数小时。现在连接 1s / 读 45s / 总 60s，且连续传输失败会
//!   打开 30s 冷却的断路器（[`service_unavailable`] 供命令层动手前预检），
//!   冷却期内一切调用立即返回可读错误（排查记录见 docs/lessons.md 第 28 节）。

use crate::infra::error::AppError;
use base64::Engine as _;
use serde_json::{json, Value};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// 服务信息（设置页展示 + 连通性测试）。
#[derive(Debug, Clone, serde::Serialize, specta::Type)]
pub struct EmbeddingServiceInfo {
    pub model: String,
    pub media_marker: String,
    /// 服务端模型隐含维度（取自 `/v1/models` 的 `meta.n_embd`，仅用于展示/校验提示）
    pub dim: usize,
    /// 是否加载了视觉塔（`/props` 的 `modalities.vision`）——为 false 时图像向量不可用
    pub vision: bool,
}

/// 向量来源：真实 HTTP 客户端与单测/e2e 的假实现都实现它，便于注入。
pub trait Embedder: Send + Sync {
    /// 连通性 + 服务信息。
    fn info(&self) -> Result<EmbeddingServiceInfo, AppError>;
    /// 图像向量：入参为已按入库策略预处理好的 JPEG 字节（见 `similarity_service::prepare_image`）。
    fn embed_image(&self, jpeg: &[u8]) -> Result<Vec<f32>, AppError>;
    /// 文本向量（二期文搜图用，与图像同一向量空间）。
    fn embed_text(&self, text: &str) -> Result<Vec<f32>, AppError>;
}

/// L2 归一化（零向量原样返回，避免除零）；归一化后点积即余弦。
pub fn l2_normalize(v: &mut [f32]) {
    let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        for x in v.iter_mut() {
            *x /= norm;
        }
    }
}

/// 从 `/embedding` 响应里取最后一条向量：`[[..], [..]]`（逐 token）与 `[..]`（已池化）都支持。
fn pick_vector(body: &Value) -> Option<Vec<f32>> {
    let emb = body.get(0)?.get("embedding")?;
    let as_vec = |v: &Value| -> Option<Vec<f32>> {
        v.as_array()?
            .iter()
            .map(|x| x.as_f64().map(|f| f as f32))
            .collect()
    };
    match emb.get(0) {
        Some(first) if first.is_array() => emb.as_array()?.iter().rev().find_map(as_vec),
        _ => as_vec(emb),
    }
}

/// 从服务端错误体里取可读信息：`{"error":{"message":"..."}}`，取不到就截断原文。
fn error_message(body: &str) -> String {
    serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|v| v.get("error")?.get("message")?.as_str().map(str::to_string))
        .unwrap_or_else(|| body.chars().take(200).collect())
}

/// 是否走假实现（e2e 测试缝）。
fn mock_enabled() -> bool {
    std::env::var("PAIM_EMBEDDING_MOCK")
        .map(|v| v == "1")
        .unwrap_or(false)
}

// —————————————————————— 断路器：服务不可用时快速失败 ——————————————————————
// 存在的理由：llama.cpp 不常驻，而索引/检索会在「服务没起」时逐条重试。
// 一台机器上被拒的连接本身也要 ~2s（实测，见 docs/lessons.md 第 28 节），
// 万级索引就是数小时空转；断路器让第 2 条之后立即失败，并由命令层据此提前中止任务。

/// 连续传输失败多少次打开断路器。一次请求内部最多重试 1 次，故 3 次 ≈ 两条请求。
const BREAKER_FAILURES: u32 = 3;
/// 冷却时长：到点后半开（下一次调用照常发请求，成功即复位）。
const BREAKER_COOLDOWN: Duration = Duration::from_secs(30);

/// 断路器状态（纯状态机，便于单测；全局实例见 [`BREAKER`]）。
struct Breaker {
    consecutive: u32,
    open_until: Option<Instant>,
    last_error: String,
}

impl Breaker {
    const fn new() -> Self {
        Self {
            consecutive: 0,
            open_until: None,
            last_error: String::new(),
        }
    }

    /// 冷却中返回可读原因（含上次错误），否则 `None`（含冷却到期后的半开）。
    fn blocked(&self, now: Instant) -> Option<String> {
        let until = self.open_until?;
        if now >= until {
            return None;
        }
        let left = (until - now).as_secs().max(1);
        Some(format!(
            "embedding 服务不可用（{left}s 后自动重试；上次错误：{}）",
            self.last_error
        ))
    }

    /// 记一次传输失败；达到阈值即进入冷却。
    fn failure(&mut self, err: &str, now: Instant) -> bool {
        self.consecutive += 1;
        self.last_error = err.to_string();
        if self.consecutive >= BREAKER_FAILURES && self.open_until.is_none() {
            self.open_until = Some(now + BREAKER_COOLDOWN);
            return true;
        }
        false
    }

    /// 记一次成功：连计数与冷却一起复位。
    fn success(&mut self) {
        self.consecutive = 0;
        self.open_until = None;
    }
}

static BREAKER: Mutex<Breaker> = Mutex::new(Breaker::new());

/// 冷却中的话直接返回可读错误（HTTP 调用入口的第一道闸）。
fn breaker_guard() -> Result<(), AppError> {
    if mock_enabled() {
        return Ok(());
    }
    match BREAKER.lock() {
        Ok(b) => match b.blocked(Instant::now()) {
            Some(msg) => Err(AppError::Message(msg)),
            None => Ok(()),
        },
        Err(_) => Ok(()), // 中毒不影响请求本身，放行
    }
}

fn breaker_failure(err: &str) {
    let Ok(mut b) = BREAKER.lock() else {
        return;
    };
    if b.failure(err, Instant::now()) {
        // 打开的那一刻记一条 WARN：release（默认 WARN 级）也能看到服务不可用这件事
        crate::log_warn!(
            "embedding 服务连续失败 {BREAKER_FAILURES} 次，进入 {}s 冷却：{err}",
            BREAKER_COOLDOWN.as_secs()
        );
    }
}

fn breaker_success() {
    if let Ok(mut b) = BREAKER.lock() {
        b.success();
    }
}

/// 服务当前是否被断路器挡住（`Some(原因)` = 挡住）。
/// 供命令层在**动手前**做一次廉价预检：例如索引开跑前、检索要现场算向量前，
/// 免得先解码/预处理再白等一次超时。
pub fn service_unavailable() -> Option<String> {
    if mock_enabled() {
        return None;
    }
    BREAKER.lock().ok().and_then(|b| b.blocked(Instant::now()))
}

/// 主动复位断路器：给「用户显式重试」的入口用（设置页「测试」、点索引按钮），
/// 冷却不该拦住明确的用户意图——自动路径（检索）不复位，才不会被反复空转拖住。
pub fn reset_breaker() {
    breaker_success();
}

/// 连通性探测（索引任务开跑前的预检）：不可用立刻返回可读错误。
pub fn probe(base_url: &str) -> Result<EmbeddingServiceInfo, AppError> {
    make_embedder(base_url).info()
}

/// 真实 HTTP 实现。
pub struct HttpEmbedder {
    base_url: String,
    agent: ureq::Agent,
}

impl HttpEmbedder {
    pub fn new(base_url: &str) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            agent: ureq::AgentBuilder::new()
                // 只连本机服务：连不上就是没起，1s 足够（旧值 3s 偏保守）
                .timeout_connect(Duration::from_secs(1))
                // 单次 I/O：图像侧是本地 CPU 编码，读超时给足；文本远快于此
                .timeout_read(Duration::from_secs(45))
                .timeout_write(Duration::from_secs(10))
                // 总预算：超过即视为服务不可用。旧值 180s 叠加重试＝一条请求能拖几分钟，
                // 服务「接受连接但不回应」时会把索引/检索按分钟计挂住
                .timeout(Duration::from_secs(60))
                .build(),
        }
    }

    fn unavailable(&self, detail: &str) -> AppError {
        AppError::Message(format!(
            "无法访问 embedding 服务 {}：{detail}（确认服务已启动、地址正确）",
            self.base_url
        ))
    }

    fn get_json(&self, path: &str) -> Result<Value, AppError> {
        breaker_guard()?;
        let url = format!("{}{path}", self.base_url);
        let resp = match self.agent.get(&url).call() {
            Ok(resp) => {
                // 服务活着（哪怕随后解析失败）→ 复位断路器
                breaker_success();
                resp
            }
            Err(e) => {
                let detail = e.to_string();
                breaker_failure(&detail);
                return Err(self.unavailable(&detail));
            }
        };
        resp.into_json::<Value>()
            .map_err(|e| AppError::Message(format!("embedding 服务返回无法解析：{e}")))
    }

    /// POST 并取回 JSON；传输层失败重试 1 次。
    /// 退避从 500ms 收到 200ms：失败路径要快（服务没起时每条都在这里白等）。
    fn post_json(&self, path: &str, body: Value) -> Result<Value, AppError> {
        let url = format!("{}{path}", self.base_url);
        let mut last = String::new();
        for attempt in 0..2 {
            breaker_guard()?;
            match self.agent.post(&url).send_json(body.clone()) {
                Ok(resp) => {
                    breaker_success();
                    return resp.into_json::<Value>().map_err(|e| {
                        AppError::Message(format!("embedding 服务返回无法解析：{e}"))
                    });
                }
                Err(ureq::Error::Status(code, resp)) => {
                    // 服务端明确拒绝（pooling 未开、图像 token 超批、多模态未加载…），重试无意义；
                    // 能回 HTTP 状态说明服务是活的 → 复位断路器
                    breaker_success();
                    let text = resp.into_string().unwrap_or_default();
                    return Err(AppError::Message(format!(
                        "embedding 服务返回 HTTP {code}：{}",
                        error_message(&text)
                    )));
                }
                Err(e) => {
                    last = e.to_string();
                    breaker_failure(&last);
                }
            }
            if attempt == 0 {
                std::thread::sleep(Duration::from_millis(200));
            }
        }
        Err(self.unavailable(&last))
    }

    /// 取本进程的 media marker（每次现取：marker 随服务重启变化，沿用旧值会静默丢图）。
    fn media_marker(&self) -> Result<String, AppError> {
        self.get_json("/props")?
            .get("media_marker")
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| AppError::Message("embedding 服务未返回 media_marker".into()))
    }
}

impl Embedder for HttpEmbedder {
    fn info(&self) -> Result<EmbeddingServiceInfo, AppError> {
        let props = self.get_json("/props")?;
        let models = self.get_json("/v1/models")?;
        let first = models.get("data").and_then(|d| d.get(0));
        Ok(EmbeddingServiceInfo {
            model: first
                .and_then(|m| m.get("id"))
                .and_then(Value::as_str)
                .unwrap_or("(未知)")
                .to_string(),
            media_marker: props
                .get("media_marker")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            dim: first
                .and_then(|m| m.get("meta"))
                .and_then(|m| m.get("n_embd"))
                .and_then(Value::as_u64)
                .unwrap_or(0) as usize,
            vision: props
                .get("modalities")
                .and_then(|m| m.get("vision"))
                .and_then(Value::as_bool)
                .unwrap_or(false),
        })
    }

    fn embed_image(&self, jpeg: &[u8]) -> Result<Vec<f32>, AppError> {
        let marker = self.media_marker()?;
        let b64 = base64::engine::general_purpose::STANDARD.encode(jpeg);
        let resp = self.post_json(
            "/embedding",
            json!({ "content": { "prompt_string": marker, "multimodal_data": [b64] } }),
        )?;
        let mut vec = pick_vector(&resp)
            .ok_or_else(|| AppError::Message("embedding 响应里没有向量".into()))?;
        l2_normalize(&mut vec);
        Ok(vec)
    }

    fn embed_text(&self, text: &str) -> Result<Vec<f32>, AppError> {
        let resp = self.post_json("/embedding", json!({ "content": text }))?;
        let mut vec = pick_vector(&resp)
            .ok_or_else(|| AppError::Message("embedding 响应里没有向量".into()))?;
        l2_normalize(&mut vec);
        Ok(vec)
    }
}

/// 伪向量的正偏置：真实 embedding 空间是各向异性的 —— 任意两条都带**小幅正**相似，
/// 因此这里的噪声叠一个 0.1 的公共分量，使任意两条伪向量相似度 ≈0.1（恒为正、远低于 0.5）。
/// 这样「阈值 0 → 全命中、默认 0.5 → 全过滤」的行为才与真服务一致（阈值判定类用例依赖它）。
const MOCK_BIAS: f32 = 0.1;

/// 假实现：由输入内容派生的确定性伪向量（同输入同向量、不同输入不同向量、已归一化）。
/// 供 Rust 单测与 e2e（`PAIM_EMBEDDING_MOCK=1`）使用，不依赖真实服务。
pub struct MockEmbedder {
    dim: usize,
}

impl Default for MockEmbedder {
    fn default() -> Self {
        Self { dim: 2048 }
    }
}

impl MockEmbedder {
    fn pseudo(&self, seed_bytes: &[u8]) -> Vec<f32> {
        use md5::{Digest, Md5};
        let digest = Md5::digest(seed_bytes);
        let mut state = u64::from_le_bytes(digest[..8].try_into().unwrap_or([0; 8]));
        let mut v = Vec::with_capacity(self.dim);
        for _ in 0..self.dim {
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            // 取 state 高 31 位映射到 [-0.5, 0.5) 再加 `MOCK_BIAS`。
            // 早先用 `u32::MAX` 归一化只到 [-0.5, 0)（均值 -0.25），任意两条伪向量的余弦恒为
            // ~0.75，会冒充「相似」——阈值判定类用例（e2e）在默认 0.5 下全命中而被带偏。
            v.push(((state >> 33) as f32 / (1u64 << 31) as f32) - 0.5 + MOCK_BIAS);
        }
        l2_normalize(&mut v);
        v
    }
}

impl Embedder for MockEmbedder {
    fn info(&self) -> Result<EmbeddingServiceInfo, AppError> {
        Ok(EmbeddingServiceInfo {
            model: "mock".into(),
            media_marker: "<__media_mock__>".into(),
            dim: self.dim,
            vision: true,
        })
    }

    fn embed_image(&self, jpeg: &[u8]) -> Result<Vec<f32>, AppError> {
        Ok(self.pseudo(jpeg))
    }

    fn embed_text(&self, text: &str) -> Result<Vec<f32>, AppError> {
        Ok(self.pseudo(text.as_bytes()))
    }
}

/// 按环境变量选择实现：`PAIM_EMBEDDING_MOCK=1` 时用假实现（e2e 测试缝）。
pub fn make_embedder(base_url: &str) -> Box<dyn Embedder> {
    if mock_enabled() {
        Box::new(MockEmbedder::default())
    } else {
        Box::new(HttpEmbedder::new(base_url))
    }
}

#[cfg(test)]
#[path = "embedding_client.test.rs"]
mod tests;
