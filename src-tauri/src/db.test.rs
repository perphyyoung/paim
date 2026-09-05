//! db 模块单元测试：数据集切换防呆的目录扫描逻辑（D4-A/D5-A 约定）。

use super::*;
use std::path::PathBuf;

fn temp_dir(name: &str) -> PathBuf {
    test_temp_dir(name)
}

#[test]
fn base_exists_is_never_pending() {
    let root = temp_dir("exists");
    let base = root.join("paim-data");
    std::fs::create_dir_all(&base).unwrap();
    std::fs::create_dir_all(root.join("paim-data.工作")).unwrap();
    assert!(pending_switch_datasets_at(&base).is_empty());
}

/// 复现 pm 备份导入的让位场景：应用运行中持有 paim.db 连接，
/// 换成内存连接释放文件锁后，数据目录应可整体改名（否则导入报 os error 5）。
#[test]
fn connection_swap_releases_db_for_dir_rename() {
    let dir = test_temp_dir("rename-after-swap");
    let bk = crate::db::init(dir.join("paim.db")).expect("init db");
    let mut guard = bk.0.lock().unwrap();

    // 与 pm_backup_service::import 一致：换成内存连接，旧连接随之关闭
    *guard = rusqlite::Connection::open_in_memory().unwrap();

    let backup = dir.parent().unwrap().join(format!(
        "{}__moved",
        dir.file_name().unwrap().to_str().unwrap()
    ));
    std::fs::rename(&dir, &backup).expect("换内存连接后应可整体改名数据目录");
    std::fs::rename(&backup, &dir).expect("改回原名");
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
