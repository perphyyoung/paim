//! logging 配置解析的单测：不触盘，只测 toml 解析与级别映射。

use super::{
    config_template, ensure_config_file, level_from_str, LogConfig, CONFIG_TEMPLATE_DEV,
    CONFIG_TEMPLATE_RELEASE,
};

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

/// 两份模板必须各自只含本环境的键（配置分离的守卫），且值都是合法级别。
#[test]
fn config_templates_are_split_per_environment() {
    let dev: LogConfig = toml::from_str(CONFIG_TEMPLATE_DEV).expect("dev 模板应可解析");
    assert_eq!(dev.dev_log_level.as_deref(), Some("debug"));
    assert_eq!(dev.release_log_level, None, "dev 模板不应出现 release 键");
    assert!(level_from_str(dev.dev_log_level.unwrap().as_str()).is_some());

    let rel: LogConfig = toml::from_str(CONFIG_TEMPLATE_RELEASE).expect("release 模板应可解析");
    assert_eq!(rel.release_log_level.as_deref(), Some("warn"));
    assert_eq!(rel.dev_log_level, None, "release 模板不应出现 dev 键");
    assert!(level_from_str(rel.release_log_level.unwrap().as_str()).is_some());

    // 当前构建类型取到的模板要与该环境一致
    let current: LogConfig = toml::from_str(config_template()).expect("当前模板应可解析");
    if cfg!(debug_assertions) {
        assert!(current.dev_log_level.is_some(), "debug 下应取 dev 模板");
        assert!(current.release_log_level.is_none());
    } else {
        assert!(
            current.release_log_level.is_some(),
            "release 下应取 release 模板"
        );
        assert!(current.dev_log_level.is_none());
    }
}

/// 自动创建语义：不存在则写模板，已存在则一个字节都不动。
#[test]
fn ensure_config_file_creates_once_and_never_overwrites() {
    let dir = crate::infra::db::test_temp_dir("logging-config");
    let path = dir.join("paim-config.toml");

    ensure_config_file(&path);
    assert_eq!(
        std::fs::read_to_string(&path).expect("模板应已写入"),
        config_template()
    );

    std::fs::write(&path, "dev-log-level = \"error\"\n").unwrap();
    ensure_config_file(&path);
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "dev-log-level = \"error\"\n",
        "已存在的配置不应被模板覆盖"
    );

    let _ = std::fs::remove_dir_all(&dir);
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
