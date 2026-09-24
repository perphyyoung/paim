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
//! - 传输层失败重试 1 次（服务端单进程易受抖动影响），HTTP 4xx/5xx 不重试，直接给可读错误。

use crate::infra::error::AppError;
use base64::Engine as _;
use serde_json::{json, Value};
use std::time::Duration;

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
                .timeout_connect(Duration::from_secs(3))
                .timeout(Duration::from_secs(180))
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
        let url = format!("{}{path}", self.base_url);
        let resp = self
            .agent
            .get(&url)
            .call()
            .map_err(|e| self.unavailable(&e.to_string()))?;
        resp.into_json::<Value>()
            .map_err(|e| AppError::Message(format!("embedding 服务返回无法解析：{e}")))
    }

    /// POST 并取回 JSON；传输层失败重试 1 次。
    fn post_json(&self, path: &str, body: Value) -> Result<Value, AppError> {
        let url = format!("{}{path}", self.base_url);
        let mut last = String::new();
        for attempt in 0..2 {
            match self.agent.post(&url).send_json(body.clone()) {
                Ok(resp) => {
                    return resp.into_json::<Value>().map_err(|e| {
                        AppError::Message(format!("embedding 服务返回无法解析：{e}"))
                    });
                }
                Err(ureq::Error::Status(code, resp)) => {
                    // 服务端明确拒绝（pooling 未开、图像 token 超批、多模态未加载…），重试无意义
                    let text = resp.into_string().unwrap_or_default();
                    return Err(AppError::Message(format!(
                        "embedding 服务返回 HTTP {code}：{}",
                        error_message(&text)
                    )));
                }
                Err(e) => last = e.to_string(),
            }
            if attempt == 0 {
                std::thread::sleep(Duration::from_millis(500));
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
    if std::env::var("PAIM_EMBEDDING_MOCK")
        .map(|v| v == "1")
        .unwrap_or(false)
    {
        Box::new(MockEmbedder::default())
    } else {
        Box::new(HttpEmbedder::new(base_url))
    }
}
