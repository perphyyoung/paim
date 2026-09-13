//! 资源管理器定位：打开指定文件所在目录并选中该文件。
//! 供「打开本地保存位置」（图像卡片/详情/关联图像右键）复用。
//!
//! 走 `SHOpenFolderAndSelectItems`（Shell 官方入口，Electron / tauri-plugin-opener 同款），
//! 不用 `explorer /select,`：后者在目标目录**尚无已打开窗口**时存在冷启动竞态——
//! 选中命令在视图创建完成前发出会被丢弃，表现为首次只打开目录、第二次才选中文件。

use crate::infra::error::AppError;
use std::path::{Path, PathBuf};
use windows::core::HSTRING;
use windows::Win32::Foundation::S_OK;
use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED};
use windows::Win32::UI::Shell::Common::ITEMIDLIST;
use windows::Win32::UI::Shell::{ILCreateFromPathW, ILFree, SHOpenFolderAndSelectItems};

/// 在资源管理器中打开 `file` 所在目录并选中 `file`。
///
/// Shell 调用失败时回退到 `explorer /select,`（旧行为：最差也能打开目录）。
pub fn reveal_in_explorer(file: &Path) -> Result<(), AppError> {
    // 先归一化分隔符：数据库 relative_path 存 `/`，`Path::join` 会保留，
    // 混用分隔符会让 `ILCreateFromPathW` 返回 null（与 explorer 回退默认位置同源）。
    let file = to_windows_path(file);
    let parent = shell_parent(&file)?;
    match select_via_shell(&parent, &file) {
        Ok(()) => Ok(()),
        Err(e) => {
            crate::log_warn!(
                "reveal_shell_failed: file={} err={e} caller=reveal_in_explorer",
                file.display()
            );
            open_with_explorer(&file)
        }
    }
}

/// 路径分隔符统一为反斜杠（Shell 只认反斜杠，见 docs/lessons.md 第 2 节）。
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

/// 官方入口定位：选中失败会返回 `Err`，不静默。
fn select_via_shell(parent: &Path, file: &Path) -> Result<(), AppError> {
    let parent_pidl = OwnedItemIdList::new(parent)?;
    let file_pidl = OwnedItemIdList::new(file)?;
    // 官方 Remarks 要求：调用线程必须先初始化 COM。
    // 只有 S_OK（本次调用完成初始化）才需要配对的 CoUninitialize；
    // S_FALSE（已初始化）/RPC_E_CHANGED_MODE（模式不同）都不能卸，否则拆掉的不是自己的初始化。
    let hr = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
    let result =
        unsafe { SHOpenFolderAndSelectItems(parent_pidl.item, Some(&[file_pidl.item]), 0) };
    if hr == S_OK {
        unsafe { CoUninitialize() };
    }
    result.map_err(|e| AppError::Message(format!("资源管理器定位失败: {e}")))
}

/// 兜底：`explorer /select,`。参数必须拆分传（`/select,` 与路径分开），
/// 否则 Explorer 回退默认位置（见 docs/lessons.md 第 2 节）。路径已在入口归一化。
fn open_with_explorer(file: &Path) -> Result<(), AppError> {
    std::process::Command::new("explorer")
        .arg("/select,")
        .arg(file)
        .spawn()
        .map_err(|e| AppError::Message(format!("打开保存位置失败: {e}")))?;
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
#[path = "shell_reveal.test.rs"]
mod tests;
