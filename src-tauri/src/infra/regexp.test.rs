//! SQLite REGEXP 标量函数测试：注册、整词、非法/超长按不匹配处理、负缓存。

use super::{register, MAX_PATTERN_LEN};
use rusqlite::Connection;

fn conn() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    register(&conn).unwrap();
    conn
}

/// `?1 REGEXP ?2` 等价 `regexp(?2, ?1)`：pattern 在前、被测文本在后
fn matches(conn: &Connection, pattern: &str, text: &str) -> bool {
    conn.query_row(
        "SELECT ?1 REGEXP ?2",
        rusqlite::params![text, pattern],
        |r| r.get::<_, bool>(0),
    )
    .unwrap()
}

#[test]
fn registers_and_matches_whole_word() {
    let conn = conn();
    assert!(matches(&conn, r"\bsole\b", "a sole b"), "独立单词应命中");
    assert!(
        !matches(&conn, r"\bsole\b", "camisole"),
        "camisole 不应命中"
    );
    assert!(!matches(&conn, r"\bsole\b", "console"), "console 不应命中");
    // 中文没有词边界概念，按子串语义写正则
    assert!(matches(&conn, "鞋", "红色鞋"));
}

#[test]
fn invalid_pattern_matches_nothing() {
    let conn = conn();
    // 语法错误不再抛错：按「不匹配」处理，查询整体得到空结果（见模块头注释）
    assert!(!matches(&conn, "(", "x"));
    // 负缓存：同一非法模式再次调用仍然是 false，且不重复解析
    assert!(!matches(&conn, "(", "x"));
}

#[test]
fn overlong_pattern_matches_nothing() {
    let conn = conn();
    let long = "a".repeat(MAX_PATTERN_LEN + 1);
    assert!(!matches(&conn, &long, long.as_str()));
}

#[test]
fn same_pattern_is_cache_hit() {
    let conn = conn();
    assert!(matches(&conn, "so.e", "sole"));
    assert!(matches(&conn, "so.e", "some"));
}
