//! logging 配置解析的单测：不触盘，只测 toml 解析与级别映射。

use super::{level_from_str, LogConfig};

#[test]
fn log_config_parses_kebab_case_keys() {
    let text = r#"
# 注释
release-log-level = "warn"
dev-log-level = "debug"
"#;
    let cfg: LogConfig = toml::from_str(text).expect("解析失败");
    assert_eq!(cfg.release_log_level.as_deref(), Some("warn"));
    assert_eq!(cfg.dev_log_level.as_deref(), Some("debug"));
}

#[test]
fn log_config_missing_keys_are_none() {
    let cfg: LogConfig = toml::from_str("").expect("空文件应解析为默认结构");
    assert_eq!(cfg.release_log_level, None);
    assert_eq!(cfg.dev_log_level, None);
}

#[test]
fn log_config_rejects_non_string_level() {
    // 值写成裸标识符（未加引号）应解析失败，由 init_from_config 走 WARN 回落
    let r: Result<LogConfig, _> = toml::from_str("release-log-level = warn");
    assert!(r.is_err());
}

#[test]
fn level_from_str_is_case_insensitive_and_rejects_unknown() {
    assert_eq!(level_from_str("DEBUG"), Some(super::Level::Debug));
    assert_eq!(level_from_str("Info"), Some(super::Level::Info));
    assert_eq!(level_from_str("warn"), Some(super::Level::Warn));
    assert_eq!(level_from_str("error"), Some(super::Level::Error));
    assert_eq!(level_from_str(""), None);
    assert_eq!(level_from_str("verbose"), None);
}
