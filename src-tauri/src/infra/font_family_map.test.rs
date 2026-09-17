use super::*;

#[test]
fn parse_normal_lines_and_quotes() {
    let map = parse_font_family_map(r#""Microsoft YaHei" = "微软雅黑""#);
    assert_eq!(
        map.get("Microsoft YaHei").map(String::as_str),
        Some("微软雅黑")
    );
}

#[test]
fn parse_skips_comments_blank_and_bad_lines() {
    let text =
        "# 注释\n\n  \nMicrosoft YaHei = 微软雅黑\nno-equal-sign\n= 只有值\n空键 = \nSimHei=黑体\n";
    let map = parse_font_family_map(text);
    assert_eq!(
        map.get("Microsoft YaHei").map(String::as_str),
        Some("微软雅黑")
    );
    assert_eq!(map.get("SimHei").map(String::as_str), Some("黑体"));
    // 坏行只影响自己，不产生残留条目
    assert!(!map.contains_key("no-equal-sign"));
    assert!(!map.contains_key(""));
    assert_eq!(map.len(), 2);
}

#[test]
fn parse_takes_first_equal_only() {
    // 中文名里出现 = 时以第一个为准
    let map = parse_font_family_map(r#""A" = "B = C""#);
    assert_eq!(map.get("A").map(String::as_str), Some("B = C"));
}

#[test]
fn parse_strips_utf8_bom() {
    let map = parse_font_family_map("\u{feff}\"SimHei\" = \"黑体\"\n");
    assert_eq!(map.get("SimHei").map(String::as_str), Some("黑体"));
}

#[test]
fn parse_duplicate_key_keeps_last() {
    let map = parse_font_family_map("\"A\" = \"一\"\n\"A\" = \"甲\"\n");
    assert_eq!(map.get("A").map(String::as_str), Some("甲"));
}

/// 内置默认模板必须能被自身解析：模板写错会让功能静默失效，这条是守卫。
#[test]
fn default_map_is_parseable_and_non_empty() {
    let map = parse_font_family_map(DEFAULT_MAP);
    assert!(!map.is_empty(), "默认模板解析后不应为空");
    assert_eq!(
        map.get("Microsoft YaHei").map(String::as_str),
        Some("微软雅黑")
    );
    assert_eq!(map.get("SimHei").map(String::as_str), Some("黑体"));
    assert_eq!(
        map.get("LXGW WenKai Screen R").map(String::as_str),
        Some("霞婺文楷")
    );
    // 模板里不应混入注释行解析出的条目
    assert!(!map.contains_key("#"));
}
