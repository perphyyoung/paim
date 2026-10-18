//! SQLite `REGEXP` 标量函数：搜索结果可选正则匹配（`X REGEXP ?` 等价 `regexp(Y, X)`）。
//!
//! 为什么用 Rust `regex` 而不是 `fancy-regex`：前者是线性时间有限自动机，不存在灾难性回溯
//! （用户输入任意正则都不会把查询拖死）；代价是**不支持 lookaround**（`(?=)` / `(?<=)`），
//! 需要「整词」时写 `\bsole\b`。
//!
//! **非法模式按「不匹配」处理**：语法错误返回 `false`，查询整体得到 0 行（列表进空态），
//! 不把错误沿 IPC 抛给前端——否则每次按键都会触发一次失败的块加载与重试，界面看起来像卡住。
//! 编译结果（含失败）按 pattern 缓存：失败不缓存会让全表每行都重新解析，失败日志也会刷屏。

use regex::Regex;
use rusqlite::{functions::FunctionFlags, Connection};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

/// 模式长度上限：超长正则只会拖慢全表扫描，不是合法用法
const MAX_PATTERN_LEN: usize = 200;
/// 编译缓存条数上限（搜索关键词通常只有一两个）
const CACHE_CAP: usize = 32;
/// 日志里 pattern 的截断长度，避免超长模式污染日志
const LOG_PATTERN_LEN: usize = 80;

/// `Some` = 编译成功；`None` = 已确认非法（负缓存，避免每行重复解析）
type Cached = Option<Regex>;

fn cache() -> &'static Mutex<HashMap<String, Cached>> {
    static CACHE: OnceLock<Mutex<HashMap<String, Cached>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn preview(pattern: &str) -> String {
    let mut s: String = pattern.chars().take(LOG_PATTERN_LEN).collect();
    if pattern.chars().count() > LOG_PATTERN_LEN {
        s.push('…');
    }
    s
}

/// 编译（或取缓存）正则；失败同样缓存，调用方据此按「不匹配」处理。
fn cached(pattern: &str) -> Result<Regex, String> {
    if pattern.chars().count() > MAX_PATTERN_LEN {
        return Err(format!("正则表达式过长（上限 {MAX_PATTERN_LEN} 字符）"));
    }
    if let Some(hit) = cache()
        .lock()
        .map_err(|e| format!("正则缓存不可用：{e}"))?
        .get(pattern)
    {
        return match hit {
            Some(re) => Ok(re.clone()),
            None => Err("正则表达式无效".to_string()),
        };
    }
    match Regex::new(pattern) {
        Ok(re) => {
            let mut map = cache().lock().map_err(|e| format!("正则缓存不可用：{e}"))?;
            if map.len() >= CACHE_CAP {
                map.clear();
            }
            map.insert(pattern.to_string(), Some(re.clone()));
            Ok(re)
        }
        Err(e) => {
            // 只在首次发现该模式非法时记一次；负缓存命中不再进这里
            crate::log_warn!("regexp_invalid: pattern={} err={e}", preview(pattern));
            let mut map = cache().lock().map_err(|e| format!("正则缓存不可用：{e}"))?;
            if map.len() >= CACHE_CAP {
                map.clear();
            }
            map.insert(pattern.to_string(), None);
            Err(format!("正则表达式无效：{e}"))
        }
    }
}

/// 预校验正则：非法 / 超长返回用户可见原因。
/// 分页接口在进入 SQL 前调用它——非法时直接返回「空结果 + 标记」，既不做全表扫描，
/// 也让前端能显示「非法正则」而不是误报「没有匹配」。
pub fn validate(pattern: &str) -> Result<(), String> {
    cached(pattern).map(|_| ())
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
            // 非法 / 超长模式 → 这条不匹配（查询整体为空），不抛错；见模块头注释
            Ok(match cached(&pattern) {
                Ok(re) => re.is_match(&text),
                Err(_) => false,
            })
        },
    )
}

#[cfg(test)]
#[path = "regexp.test.rs"]
mod tests;
