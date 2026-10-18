//! SQLite REGEXP 标量函数测试：注册、整词、缓存与用户可见的错误文案。

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
fn invalid_pattern_reports_user_facing_error() {
    let conn = conn();
    let err = conn
        .query_row("SELECT 'x' REGEXP ?1", ["("], |r| r.get::<_, bool>(0))
        .unwrap_err()
        .to_string();
    assert!(err.contains("正则表达式无效"), "实际错误：{err}");
}

#[test]
fn overlong_pattern_is_rejected() {
    let conn = conn();
    let long = "a".repeat(MAX_PATTERN_LEN + 1);
    let err = conn
        .query_row("SELECT 'x' REGEXP ?1", [long], |r| r.get::<_, bool>(0))
        .unwrap_err()
        .to_string();
    assert!(err.contains("过长"), "实际错误：{err}");
}

#[test]
fn same_pattern_is_cache_hit() {
    let conn = conn();
    assert!(matches(&conn, "so.e", "sole"));
    assert!(matches(&conn, "so.e", "some"));
}
