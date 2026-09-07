//! 文本处理工具函数。
//! 与数据库/schema 无关，供后端各处的字符串处理复用。

/// 转义 SQLite LIKE 通配符：\ % _ 视作字面量，转义符为反斜杠。
/// 必须与 SQL 中的 `LIKE ? ESCAPE '\'` 配对使用，
/// 否则用户输入单个 % 会匹配全部、_ 匹配任意单字符。
/// 图像/提示词（如有）搜索统一复用，避免各仓库各自实现。
pub fn escape_like(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

#[cfg(test)]
#[path = "text_utils.test.rs"]
mod tests;
