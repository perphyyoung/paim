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

/// 搜索说明浮层（`src/components/SearchHelpPopover.vue`）「已支持」表的可执行版本：
/// 每个写法都配**正例 + 反例**（反例专门验证不会误命中），改文案必须同步本表。
struct Example {
    pattern: &'static str,
    /// 应当命中
    hit: &'static str,
    /// 不应命中（只测正例的话，`contains` 与「整词」这类差异会被漏掉）
    miss: &'static str,
}

const SUPPORTED_EXAMPLES: &[Example] = &[
    Example {
        pattern: r"\bsole\b",
        hit: "a sole b",
        miss: "camisole",
    },
    Example {
        pattern: r"(裙|dress)",
        hit: "红色裙",
        miss: "蓝色上衣",
    },
    Example {
        pattern: r"gr[ae]y",
        hit: "grey",
        miss: "groy",
    },
    Example {
        pattern: r"colou?r",
        hit: "color",
        miss: "colouur",
    },
    Example {
        pattern: r"^e2e",
        hit: "e2e x",
        miss: "x e2e",
    },
    Example {
        pattern: r"(?i)sole",
        hit: "SOLE",
        miss: "solo",
    },
    Example {
        pattern: r"\p{Han}+",
        hit: "半身裙",
        miss: "abc",
    },
    Example {
        pattern: "半身裙",
        hit: "红色半身裙",
        miss: "连衣裙",
    },
];

/// 搜索说明浮层「明确不支持」的写法：必须编译失败（UI 提示「非法正则表达式」）
const UNSUPPORTED_EXAMPLES: &[&str] = &[r"sole(?!s)", r"(a)\1", r"(?(1)a|b)", r"(?#c)"];

#[test]
fn help_examples_match_documented_behavior() {
    let conn = conn();
    for ex in SUPPORTED_EXAMPLES {
        assert!(
            matches(&conn, ex.pattern, ex.hit),
            "示例「{}」应命中「{}」",
            ex.pattern,
            ex.hit
        );
        assert!(
            !matches(&conn, ex.pattern, ex.miss),
            "示例「{}」不应命中「{}」",
            ex.pattern,
            ex.miss
        );
    }
    for &pattern in UNSUPPORTED_EXAMPLES {
        assert!(
            super::validate(pattern).is_err(),
            "说明标注不支持，但编译通过：{pattern}"
        );
        assert!(
            !matches(&conn, pattern, "任意文本"),
            "非法模式不应命中：{pattern}"
        );
    }
}
