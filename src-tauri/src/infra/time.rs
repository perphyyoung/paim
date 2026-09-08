//! 时间处理工具函数。
//! 与业务无关的纯函数，供后端各处的时间解析/规整复用。

use chrono::TimeZone;

/// 把输入的时间字符串规整为 ISO 8601 UTC 字符串（如 `2026-09-08T04:13:54.348Z`）。
///
/// 支持两种输入：
/// - RFC3339/ISO 8601（paim 原生 / pm 新版）：原样规整为 `Z` 结尾的毫秒精度格式；
/// - 本地斜杠格式 `2026/4/15 22:40:24`（pm 早期备份）：按本地墙钟解析后转 UTC，显示时间不变。
///
/// 统一归一使库内时间字段格式一致：SQL `ORDER BY` 的字符串序、前端字符串/时间戳排序才能
/// 正确，也避免多种格式混存。无法解析时保留原值返回（由调用方决定容错策略）。
pub fn normalize_ts(raw: &str) -> String {
    // 已是 RFC3339（ISO 8601）：规整成 `Z` 结尾的毫秒格式
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(raw) {
        return dt
            .with_timezone(&chrono::Utc)
            .format("%Y-%m-%dT%H:%M:%S%.3fZ")
            .to_string();
    }
    // 本地斜杠格式 `2026/4/15 22:40:24`：按本地墙钟解析后转 UTC
    if let Ok(naive) = chrono::NaiveDateTime::parse_from_str(raw, "%Y/%m/%d %H:%M:%S") {
        let local = match chrono::Local.from_local_datetime(&naive) {
            chrono::LocalResult::Single(dt) | chrono::LocalResult::Ambiguous(dt, _) => dt,
            chrono::LocalResult::None => return raw.to_string(),
        };
        return local
            .with_timezone(&chrono::Utc)
            .format("%Y-%m-%dT%H:%M:%S%.3fZ")
            .to_string();
    }
    raw.to_string()
}

#[cfg(test)]
#[path = "time.test.rs"]
mod tests;
