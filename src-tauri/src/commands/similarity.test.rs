//! commands::similarity 的单元测试：开跑前的维度探测预检。
//! 只钉「该中止的给原因、不该中止的放行」——命令主体涉及线程与真实服务请求，由 e2e 覆盖。

use super::*;
use crate::infra::embedding_client::{Embedder, MockEmbedder};

#[test]
fn probe_dim_abort_reason_only_fires_with_baseline_and_pending() {
    // 无基准（本表还没有在用向量）：任何新维度都算一致 → 放行
    assert!(probe_dim_abort_reason(8, None, true).is_none());
    // 无待处理：即便维度不符也不该中止（没有条目要建向量）
    assert!(probe_dim_abort_reason(8, Some(8), false).is_none());
    assert!(probe_dim_abort_reason(2048, Some(8), false).is_none());
    // 维度一致：放行
    assert!(probe_dim_abort_reason(2048, Some(2048), true).is_none());
    // 维度不符且有待处理：给出可读原因，措辞与逐条兜底同一来源
    let reason = probe_dim_abort_reason(2048, Some(8), true).expect("维度不符应中止");
    assert!(reason.contains("向量维度 2048"), "{reason}");
    assert!(reason.contains("与本表已存维度 8 不一致"), "{reason}");
    assert!(reason.contains("清掉旧维度后重试「增量索引」"), "{reason}");
}

#[test]
fn probe_image_dim_abort_reason_compares_probed_dim_against_baseline() {
    // 探测图真跑一次：MockEmbedder 实际返回 2048 维
    let mock = MockEmbedder::default();
    let embedder: &dyn Embedder = &mock;
    // 基准一致 → 放行
    assert!(probe_image_dim_abort_reason(embedder, Some(2048), true)
        .unwrap()
        .is_none());
    // 基准不符 → 中止，原因里带实测维度与旧维度
    let reason = probe_image_dim_abort_reason(embedder, Some(8), true)
        .unwrap()
        .expect("维度不符应中止");
    assert!(reason.contains("向量维度 2048"), "{reason}");
    assert!(reason.contains("已存维度 8"), "{reason}");
    // 无基准 / 无待处理：连探测都不发，直接放行
    assert!(probe_image_dim_abort_reason(embedder, None, true)
        .unwrap()
        .is_none());
    assert!(probe_image_dim_abort_reason(embedder, Some(8), false)
        .unwrap()
        .is_none());
}
