//! 文本工具单元测试：SQLite LIKE 通配符转义。

use super::*;

#[test]
fn escape_like_passes_plain_text_through() {
    assert_eq!(escape_like("风景"), "风景");
    assert_eq!(escape_like("a-b 123"), "a-b 123");
}

#[test]
fn escape_like_escapes_percent() {
    assert_eq!(escape_like("%"), "\\%");
    assert_eq!(escape_like("50%off"), "50\\%off");
}

#[test]
fn escape_like_escapes_underscore() {
    assert_eq!(escape_like("_"), "\\_");
    assert_eq!(escape_like("a_b"), "a\\_b");
}

#[test]
fn escape_like_escapes_backslash_first() {
    assert_eq!(escape_like("\\"), "\\\\");
    assert_eq!(escape_like("a\\%b"), "a\\\\\\%b");
}
