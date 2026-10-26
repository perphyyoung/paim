//! 主页列表查询条件（分页 + 排序 + 筛选）：图像与提示词两域共用同一组语义与常量。
//! 前端按块拉取（块大小 = limit），排序与筛选全部下推到 SQL——分页后拿不到全量，
//! 内存里无法再排序 / 过滤，因此这里是两主页列表的唯一入口。
//!
//! 特殊标签名与前端 `src/features/tag/specialTags.ts` 的 `SPECIAL_TAG_NAMES` 一一对应，
//! 改任一处都必须同步另一处（否则筛选会退化成「按普通标签名查不到」）。

use rusqlite::types::Value;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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
pub const SP_NO_VEC: &str = "无向";
pub const SP_SINGLE_LANG: &str = "单语";
pub const SP_SAFE: &str = "安全";
pub const SP_UNSAFE: &str = "敏感";

/// 搜索匹配模式：默认子串 LIKE；`Regex` 走 SQLite 自定义 REGEXP 函数（见 `infra::regexp`）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub enum SearchMode {
    #[default]
    Substring,
    Regex,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct ListQuery {
    pub offset: i64,
    pub limit: i64,
    /// 搜索关键词（空串表示不过滤）
    pub search: String,
    /// 搜索模式（缺省按子串，兼容旧前端）
    #[serde(default)]
    pub search_mode: SearchMode,
    /// 所选标签（含特殊标签名），多标签为 AND
    pub tags: Vec<String>,
    /// 反选：排除同时命中全部所选标签的条目
    pub inverted: bool,
    /// 排序键（驼峰，与前端 sortBy 一致）；未命中白名单时回落默认键
    pub sort: String,
    pub desc: bool,
    /// 随机排序种子（sort == "random" 时生效）：同 seed 同集合得同一全序，保证分页稳定
    #[serde(default)]
    pub seed: Option<i64>,
}

/// 是否随机排序（"random" 不进 sort_column 白名单，走专门的洗牌分支）
pub fn is_random(sort: &str) -> bool {
    sort == "random"
}

fn splitmix64(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = x;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// (seed, id) 的稳定哈希：同输入恒定（跨进程 / 跨请求），不同 seed 打乱顺序
fn keyed_hash(seed: u64, id: &str) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325u64 ^ (seed as u64);
    for b in id.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01B3);
    }
    splitmix64(h)
}

/// 随机排序：按 (seed, id) 哈希升序的全序排列。
/// 同 seed 同集合必得同一顺序（分页 / 重复请求稳定）；desc 为整体反转。
pub fn shuffle_ids_seeded(mut ids: Vec<String>, seed: i64, desc: bool) -> Vec<String> {
    let seed = seed as u64;
    let mut keyed: Vec<(u64, String)> = ids
        .drain(..)
        .map(|id| (keyed_hash(seed, &id), id))
        .collect();
    keyed.sort_by(|a, b| a.0.cmp(&b.0));
    if desc {
        keyed.reverse();
    }
    keyed.into_iter().map(|(_, id)| id).collect()
}

/// 随机排序：取筛选后的全部 id（稳定序，交给洗牌函数打乱）
fn fetch_ids(
    conn: &Connection,
    table: &str,
    filter: &str,
    params: &[Value],
) -> rusqlite::Result<Vec<String>> {
    let sql = format!("SELECT id FROM {table} WHERE is_deleted = 0{filter} ORDER BY id");
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(rusqlite::params_from_iter(params.iter()), |r| r.get(0))?;
    rows.collect()
}

/// 随机排序的 id 列表（批量全选 / 反选用）：洗牌后截断到 MAX_IDS，与分页同序
pub fn random_ids(
    conn: &Connection,
    q: &ListQuery,
    table: &str,
    filter: &str,
    params: &[Value],
) -> rusqlite::Result<Vec<String>> {
    let ids = fetch_ids(conn, table, filter, params)?;
    let shuffled = shuffle_ids_seeded(ids, q.seed.unwrap_or(0), q.desc);
    Ok(shuffled.into_iter().take(MAX_IDS as usize).collect())
}

/// 随机排序的分页：取筛选后的 id → 洗牌 → 切片 → 按序取卡片。
/// 同 seed 同筛选必得同一全序，分页切片拼回即全量（无重 / 无漏）。
/// `row_mapper` 返回 (id, 卡片)：按 id 建索引后再按洗牌顺序拼回。
pub fn random_paged<T, P>(
    conn: &Connection,
    q: &ListQuery,
    table: &str,
    cols: &str,
    filter: &str,
    params: &[Value],
    row_mapper: &impl Fn(&rusqlite::Row<'_>) -> rusqlite::Result<(String, T)>,
    build: impl FnOnce(Vec<T>, i64, bool) -> P,
) -> rusqlite::Result<P> {
    let ids = fetch_ids(conn, table, filter, params)?;
    let total = ids.len() as i64;
    let page_ids: Vec<String> = shuffle_ids_seeded(ids, q.seed.unwrap_or(0), q.desc)
        .into_iter()
        .skip(clamp_offset(q.offset) as usize)
        .take(clamp_limit(q.limit) as usize)
        .collect();

    let mut by_id: HashMap<String, T> = HashMap::new();
    for chunk in page_ids.chunks(500) {
        let placeholders = vec!["?"; chunk.len()].join(",");
        let sql = format!("SELECT {cols} FROM {table} WHERE id IN ({placeholders})");
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(chunk.iter()), row_mapper)?;
        for row in rows {
            let (id, item) = row?;
            by_id.insert(id, item);
        }
    }
    let items = page_ids.iter().filter_map(|id| by_id.remove(id)).collect();
    Ok(build(items, total, false))
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
