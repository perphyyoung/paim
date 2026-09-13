use super::*;

/// 无父目录（相对路径/盘符根）时应在动 Shell 之前就报错，
/// 否则 `SHOpenFolderAndSelectItems` 会失败并触发兜底弹资源管理器窗口。
#[test]
fn reveal_without_parent_fails_before_shell() {
    let err = reveal_in_explorer(Path::new("no-parent.txt")).unwrap_err();
    assert!(err.to_string().contains("无法取得父目录"), "{err}");
}

/// 回归：relative_path 用 `/` 存储、`Path::join` 会保留，
/// 未归一化的混用分隔符会让 `ILCreateFromPathW` 返回 null（表现为定位失败、只打开目录）。
#[test]
fn mixed_separators_are_normalized() {
    assert_eq!(
        to_windows_path(Path::new("D:/data/images/202607/a.webp")),
        PathBuf::from("D:\\data\\images\\202607\\a.webp")
    );
}
