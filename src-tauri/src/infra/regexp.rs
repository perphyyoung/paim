//! SQLite `REGEXP` 标量函数：搜索结果可选正则匹配（`X REGEXP ?` 等价 `regexp(Y, X)`）。
//!
//! 为什么用 Rust `regex` 而不是 `fancy-regex`：前者是线性时间有限自动机，不存在灾难性回溯
//! （用户输入任意正则都不会把查询拖死）；代价是**不支持 lookaround**（`(?=)` / `(?<=)`），
//! 需要「整词」时写 `\bsole\b`。编译结果按 pattern 缓存：同一关键词要在多个列上各调一次函数，
//! 不缓存会重复编译。
//!
//! 所有连接都必须经过 [`crate::infra::db::open_connection`]，函数在那里统一注册。

use regex::Regex;
use rusqlite::{functions::FunctionFlags, Connection};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

/// 模式长度上限：超长正则只会拖慢全表扫描，不是合法用法
const MAX_PATTERN_LEN: usize = 200;
/// 编译缓存条数上限（搜索关键词通常只有一两个）
const CACHE_CAP: usize = 32;

fn cache() -> &'static Mutex<HashMap<String, Regex>> {
    static CACHE: OnceLock<Mutex<HashMap<String, Regex>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// 编译（或取缓存）正则；错误文案直接面向用户
fn cached(pattern: &str) -> Result<Regex, String> {
    if pattern.chars().count() > MAX_PATTERN_LEN {
        return Err(format!("正则表达式过长（上限 {MAX_PATTERN_LEN} 字符）"));
    }
    if let Some(re) = cache()
        .lock()
        .map_err(|e| format!("正则缓存不可用：{e}"))?
        .get(pattern)
    {
        return Ok(re.clone());
    }
    let re = Regex::new(pattern).map_err(|e| format!("正则表达式无效：{e}"))?;
    let mut map = cache().lock().map_err(|e| format!("正则缓存不可用：{e}"))?;
    if map.len() >= CACHE_CAP {
        map.clear();
    }
    map.insert(pattern.to_string(), re.clone());
    Ok(re)
}

/// 注册 `regexp(pattern, text)`：SQL 里写 `text REGEXP ?` 即可（SQLite 把操作数反转后调用）。
pub fn register(conn: &Connection) -> rusqlite::Result<()> {
    conn.create_scalar_function(
        "regexp",
        2,
        FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
        |ctx| {
            let pattern: String = ctx.get(0)?;
            let text: String = ctx.get(1)?;
            let re = cached(&pattern).map_err(|e| {
                rusqlite::Error::UserFunctionError(
                    std::io::Error::new(std::io::ErrorKind::InvalidInput, e).into(),
                )
            })?;
            Ok(re.is_match(&text))
        },
    )
}

#[cfg(test)]
#[path = "regexp.test.rs"]
mod tests;
