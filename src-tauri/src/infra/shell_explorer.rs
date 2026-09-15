//! 资源管理器操作：打开目录 / 打开目录并选中文件。
//! 供「打开数据目录」与「打开本地保存位置」（图像卡片/详情/关联图像右键）复用。
//!
//! 走 Shell API 而非 `explorer` 命令行：后者在目标目录**尚无已打开窗口**时存在冷启动竞态——
//! 选中命令在视图创建完成前发出会被丢弃，表现为首次只打开目录、第二次才选中文件。

use crate::infra::error::AppError;
use std::path::{Path, PathBuf};
use windows::core::{w, HSTRING, PCWSTR};
use windows::Win32::Foundation::S_OK;
use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED};
use windows::Win32::UI::Shell::Common::ITEMIDLIST;
use windows::Win32::UI::Shell::{
    ILCreateFromPathW, ILFree, SHOpenFolderAndSelectItems, ShellExecuteExW, SHELLEXECUTEINFOW,
};
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

/// 在资源管理器中打开目录 `dir`（不选中任何项）。
///
/// Shell 调用失败时回退到 `explorer <dir>`。
pub fn open_in_explorer(dir: &Path) -> Result<(), AppError> {
    let dir = to_windows_path(dir);
    match open_folder_via_shell(&dir) {
        Ok(()) => Ok(()),
        Err(e) => {
            crate::log_warn!(
                "open_shell_failed: dir={} err={e} caller=open_in_explorer",
                dir.display()
            );
            spawn_explorer(false, &dir)
        }
    }
}

/// 在资源管理器中打开 `file` 所在目录并选中 `file`。
///
/// Shell 调用失败时回退到 `explorer /select,`（旧行为：最差也能打开目录）。
pub fn reveal_in_explorer(file: &Path) -> Result<(), AppError> {
    let file = to_windows_path(file);
    let parent = shell_parent(&file)?;
    match select_via_shell(&parent, &file) {
        Ok(()) => Ok(()),
        Err(e) => {
            crate::log_warn!(
                "reveal_shell_failed: file={} err={e} caller=reveal_in_explorer",
                file.display()
            );
            spawn_explorer(true, &file)
        }
    }
}

/// 路径分隔符统一为反斜杠：Shell 只认反斜杠，
/// 混用分隔符会让 `ILCreateFromPathW` 返回 null、`explorer` 回退默认位置
/// （数据库 `relative_path` 存 `/`，`Path::join` 会保留，见 docs/lessons.md 第 2 节）。
fn to_windows_path(path: &Path) -> PathBuf {
    PathBuf::from(path.to_string_lossy().replace('/', "\\"))
}

/// 取用于定位的父目录（相对路径等无父目录的场景直接报错，不惊动资源管理器）。
fn shell_parent(file: &Path) -> Result<PathBuf, AppError> {
    match file.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => Ok(parent.to_path_buf()),
        _ => Err(AppError::Message(format!(
            "无法取得父目录: {}",
            file.display()
        ))),
    }
}

/// 打开目录：`ShellExecuteExW` + `explore` 动词（Shell 官方入口，tauri-plugin-opener 同款）。
fn open_folder_via_shell(dir: &Path) -> Result<(), AppError> {
    let path = HSTRING::from(dir);
    let mut info = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        nShow: SW_SHOWNORMAL.0,
        lpVerb: w!("explore"),
        lpFile: PCWSTR(path.as_ptr()),
        ..Default::default()
    };
    with_com(|| unsafe { ShellExecuteExW(&mut info) })
        .map_err(|e| AppError::Message(format!("资源管理器打开失败: {e}")))
}

/// 定位并选中：官方入口，失败返回 `Err` 不静默。
fn select_via_shell(parent: &Path, file: &Path) -> Result<(), AppError> {
    let parent_pidl = OwnedItemIdList::new(parent)?;
    let file_pidl = OwnedItemIdList::new(file)?;
    with_com(|| unsafe { SHOpenFolderAndSelectItems(parent_pidl.item, Some(&[file_pidl.item]), 0) })
        .map_err(|e| AppError::Message(format!("资源管理器定位失败: {e}")))
}

/// Shell API 要求调用线程先初始化 COM（官方 Remarks）。
///
/// 只有 `S_OK`（本次调用完成初始化）才需要配对的 `CoUninitialize`；
/// `S_FALSE`（已初始化）/`RPC_E_CHANGED_MODE`（模式不同）都不能卸，否则拆掉的不是自己的初始化。
fn with_com<T>(f: impl FnOnce() -> T) -> T {
    let hr = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
    let result = f();
    if hr == S_OK {
        unsafe { CoUninitialize() };
    }
    result
}

/// 兜底：`explorer`（路径已在入口归一化）。`/select,` 必须单独成一个参数传，
/// 否则 Explorer 回退默认位置（见 docs/lessons.md 第 2 节）。
fn spawn_explorer(select: bool, path: &Path) -> Result<(), AppError> {
    let mut cmd = std::process::Command::new("explorer");
    if select {
        cmd.arg("/select,");
    }
    cmd.arg(path)
        .spawn()
        .map_err(|e| AppError::Message(format!("打开资源管理器失败: {e}")))?;
    Ok(())
}

/// PIDL 及其源字符串：`ILCreateFromPathW` 返回的 PIDL 只引用路径字符串，
/// 因此两者必须同生命周期（同结构体持有），并在 drop 时 `ILFree`。
struct OwnedItemIdList {
    _path: HSTRING,
    item: *const ITEMIDLIST,
}

impl OwnedItemIdList {
    fn new(path: &Path) -> Result<Self, AppError> {
        let path_hstring = HSTRING::from(path);
        let item = unsafe { ILCreateFromPathW(&path_hstring) };
        if item.is_null() {
            return Err(AppError::Message(format!(
                "路径无法转为 Shell PIDL: {}",
                path.display()
            )));
        }
        Ok(Self {
            _path: path_hstring,
            item,
        })
    }
}

impl Drop for OwnedItemIdList {
    fn drop(&mut self) {
        unsafe { ILFree(Some(self.item)) };
    }
}

#[cfg(test)]
#[path = "shell_explorer.test.rs"]
mod tests;
