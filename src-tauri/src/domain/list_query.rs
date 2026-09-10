//! 主页列表查询条件（分页 + 排序 + 筛选）：图像与提示词两域共用同一组语义与常量。
//! 前端按块拉取（块大小 = limit），排序与筛选全部下推到 SQL——分页后拿不到全量，
//! 内存里无法再排序 / 过滤，因此这里是两主页列表的唯一入口。
//!
//! 特殊标签名与前端 `src/features/tag/specialTags.ts` 的 `SPECIAL_TAG_NAMES` 一一对应，
//! 改任一处都必须同步另一处（否则筛选会退化成「按普通标签名查不到」）。

use rusqlite::types::Value;
use serde::{Deserialize, Serialize};

/// 单块最多返回条数（前端块大小上限，防误传导致一次拉全表）
pub const MAX_LIMIT: i64 = 200;
/// `list_*_ids` 单次上限（全选 / 反选 / 批量用；万级数据 1 万 id ≈ 360 KB）
pub const MAX_IDS: i64 = 20_000;

// —— 特殊标签名（虚拟筛选，不是真实标签记录）——
pub const SP_FAVORITE: &str = "收藏";
pub const SP_UNREFERENCED: &str = "未引";
pub const SP_MULTI_REF: &str = "多引";
pub const SP_MULTI_IMAGE: &str = "多图";
pub const SP_NO_IMAGE: &str = "无图";
pub const SP_NO_TAG: &str = "无标";
pub const SP_SINGLE_LANG: &str = "单语";
pub const SP_SAFE: &str = "安全";
pub const SP_UNSAFE: &str = "敏感";

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct ListQuery {
    pub offset: i64,
    pub limit: i64,
    /// 搜索关键词（空串表示不过滤）
    pub search: String,
    /// 所选标签（含特殊标签名），多标签为 AND
    pub tags: Vec<String>,
    /// 反选：排除同时命中全部所选标签的条目
    pub inverted: bool,
    /// 排序键（驼峰，与前端 sortBy 一致）；未命中白名单时回落默认键
    pub sort: String,
    pub desc: bool,
}

/// 排序方向：只可能是这两个字面量，禁止把用户输入拼进 SQL
pub fn order_dir(desc: bool) -> &'static str {
    if desc {
        "DESC"
    } else {
        "ASC"
    }
}

/// 块大小钳制到 [1, MAX_LIMIT]
pub fn clamp_limit(limit: i64) -> i64 {
    limit.clamp(1, MAX_LIMIT)
}

/// offset 负值按 0 处理
pub fn clamp_offset(offset: i64) -> i64 {
    offset.max(0)
}

/// 把字符串参数批量转成 SQL 绑定值
pub fn to_values(values: Vec<String>) -> Vec<Value> {
    values.into_iter().map(Value::Text).collect()
}
