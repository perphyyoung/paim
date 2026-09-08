use super::*;

#[test]
fn normalize_ts_canonicalizes_iso_and_slash() {
    // ISO 输入（paim 原生 / pm 新版）应原样规整为 Z 结尾的毫秒格式
    assert_eq!(
        normalize_ts("2026-09-08T04:13:54.348Z"),
        "2026-09-08T04:13:54.348Z"
    );
    // 本地斜杠格式（pm 早期备份）应转为合法 RFC3339 且以 Z 结尾
    let got = normalize_ts("2026/4/15 22:40:24");
    assert!(got.ends_with('Z'), "斜杠格式应转成 Z 结尾的 ISO: {got}");
    assert!(
        got.starts_with("2026-04-15T"),
        "斜杠本地时间转 UTC 后日期应为 2026-04-15: {got}"
    );
    assert!(
        chrono::DateTime::parse_from_rfc3339(&got).is_ok(),
        "输出必须是合法 RFC3339: {got}"
    );
    // 无法识别的格式保留原值（不影响导入）
    assert_eq!(normalize_ts("not-a-time"), "not-a-time");
}
