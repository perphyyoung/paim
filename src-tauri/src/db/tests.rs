//! db 模块单元测试：数据集切换防呆的目录扫描逻辑（D4-A/D5-A 约定）。

use super::*;
use std::path::PathBuf;

fn temp_dir(name: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("paim-db-test-{name}-{nanos}"));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn base_exists_is_never_pending() {
    let root = temp_dir("exists");
    let base = root.join("paim-data");
    std::fs::create_dir_all(&base).unwrap();
    std::fs::create_dir_all(root.join("paim-data.工作")).unwrap();
    assert!(pending_switch_datasets_at(&base).is_empty());
}

#[test]
fn missing_base_with_datasets_is_pending() {
    let root = temp_dir("pending");
    let base = root.join("paim-data");
    std::fs::create_dir_all(root.join("paim-data.测试")).unwrap();
    std::fs::create_dir_all(root.join("paim-data.工作")).unwrap();
    // 结果按 UTF-8 字节序稳定排序
    assert_eq!(
        pending_switch_datasets_at(&base),
        vec!["paim-data.工作", "paim-data.测试"]
    );
}

#[test]
fn missing_base_without_datasets_is_first_launch() {
    let root = temp_dir("first-launch");
    let base = root.join("paim-data");
    assert!(pending_switch_datasets_at(&base).is_empty());
}

#[test]
fn non_directory_entries_are_ignored() {
    let root = temp_dir("noise");
    let base = root.join("paim-data");
    std::fs::write(root.join("paim-data.txt"), b"x").unwrap();
    std::fs::create_dir_all(root.join("other")).unwrap();
    std::fs::create_dir_all(root.join("paim-dataX")).unwrap();
    assert!(pending_switch_datasets_at(&base).is_empty());
}
