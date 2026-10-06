//! embedding 客户端纯逻辑单测：断路器状态机、响应解析、归一化、错误体提取。
//! 全部不触网——真实 HTTP 行为由 e2e 的假 embedding（`PAIM_EMBEDDING_MOCK`）与手测覆盖。

use super::*;

#[test]
fn breaker_opens_only_after_consecutive_failures() {
    let now = Instant::now();
    let mut b = Breaker::new();
    // 未达阈值：只累计，不拦
    assert!(b.blocked(now).is_none());
    assert!(!b.failure("e1", now));
    assert!(b.blocked(now).is_none());
    assert!(!b.failure("e2", now));
    assert!(b.blocked(now).is_none());
    // 达阈值：打开，且原因里带上次错误（可读性是这套闸门的一半价值）
    assert!(b.failure("e3", now), "达到阈值应打开断路器");
    let msg = b.blocked(now).expect("阈值后应被拦住");
    assert!(msg.contains("embedding 服务不可用"), "{msg}");
    assert!(msg.contains("e3"), "{msg}");
}

#[test]
fn breaker_half_opens_after_cooldown_and_resets_on_success() {
    let now = Instant::now();
    let mut b = Breaker::new();
    for _ in 0..BREAKER_FAILURES {
        b.failure("boom", now);
    }
    assert!(b.blocked(now).is_some());
    // 冷却到期后半开：不再拦（下一次调用照常发请求）
    assert!(b.blocked(now + BREAKER_COOLDOWN).is_none());
    // 成功即复位：连计数与冷却一起清空
    b.success();
    assert!(b.blocked(now).is_none());
    assert!(!b.failure("again", now), "复位后应从头计数，不该立刻再打开");
}

#[test]
fn pick_vector_accepts_pooled_and_per_token_shapes() {
    // 已池化：直接一条
    let pooled = serde_json::json!([{ "embedding": [0.25, 0.5] }]);
    assert_eq!(pick_vector(&pooled), Some(vec![0.25, 0.5]));
    // 逐 token：取最后一条（Qwen3 系以最后一个 token 为句向量）
    let per_token = serde_json::json!([{ "embedding": [[1.0, 0.0], [0.0, 1.0]] }]);
    assert_eq!(pick_vector(&per_token), Some(vec![0.0, 1.0]));
    // 空响应 / 非数值内容：一律 None（调用方据此报「响应里没有向量」）
    assert_eq!(pick_vector(&serde_json::json!([])), None);
    assert_eq!(
        pick_vector(&serde_json::json!([{ "embedding": ["x"] }])),
        None
    );
}

#[test]
fn l2_normalize_keeps_zero_vector_and_scales_others() {
    let mut zero = [0.0f32, 0.0];
    l2_normalize(&mut zero);
    assert_eq!(zero, [0.0, 0.0], "零向量原样返回，不能除零产出 NaN");

    let mut v = [3.0f32, 4.0];
    l2_normalize(&mut v);
    assert!(
        (v[0] - 0.6).abs() < 1e-6 && (v[1] - 0.8).abs() < 1e-6,
        "{v:?}"
    );
}

#[test]
fn error_message_prefers_server_message() {
    assert_eq!(
        error_message(r#"{"error":{"message":"context too long"}}"#),
        "context too long"
    );
    // 非 JSON 错误体：截断原文兜底
    assert_eq!(error_message("plain text"), "plain text");
}
