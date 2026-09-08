//! 备份服务共享工具：ZIP 条目定位/读取、安全路径校验、临时目录、目录递归复制。
//! pm 备份导入（pm_backup_service）与 paim 自有备份导出/导入（paim_backup_service）共用。

use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::io::Read;
use std::path::{Path, PathBuf};
use zip::ZipArchive;

use crate::infra::db;

/// 备份包内 manifest 的固定条目名（两种备份包布局一致）。
pub(crate) const MANIFEST_ENTRY: &str = "manifest.json";

/// 备份内容概览（inspect 返回，供确认弹窗展示；pm/paim 共用）。
/// `app` 为归一化来源标识：`"paim"` 或 `"pm"`（由 manifest appName 探测）。
#[derive(Debug, Serialize, specta::Type)]
pub struct BackupInfo {
    pub app: String,
    pub exported_at: String,
    pub prompt_count: i64,
    pub image_count: i64,
    pub trashed_prompt_count: i64,
    pub trashed_image_count: i64,
    pub prompt_tag_count: i64,
    pub image_tag_count: i64,
}

/// 备份导出结果摘要。
#[derive(Debug, Serialize, specta::Type)]
pub struct BackupExportSummary {
    pub prompts: i64,
    pub images: i64,
    pub file_path: String,
}

/// 备份导入结果摘要（pm/paim 共用）。
#[derive(Debug, Serialize, specta::Type)]
pub struct BackupImportSummary {
    pub prompts: i64,
    pub images: i64,
    pub thumbnail_failures: usize,
    /// 原数据目录的备份位置（整体改名让位）；无原数据时为空串。
    pub backup_dir: String,
}

/// 备份导出/导入进度推送载荷（pm 与 paim 备份共用同一事件通道 backup-progress；
/// 两类操作互斥于设置页且监听只在各自命令执行期间挂载，单通道无串扰）。
#[derive(Debug, Serialize, Deserialize, Clone, specta::Type, tauri_specta::Event)]
#[tauri_specta(event_name = "backup-progress")]
pub struct BackupProgress {
    pub stage: String,
    pub percent: u32,
    pub status: String,
    pub detail: Option<String>,
}

/// 条目相对路径是否安全（拒绝空段/./.. /盘符），防 zip-slip。
pub(crate) fn is_safe_rel_path(rel: &str) -> bool {
    !rel.split('/')
        .any(|seg| seg.is_empty() || seg == "." || seg == ".." || seg.ends_with(':'))
}

/// 在系统临时目录下建当次备份操作的解压/暂存目录（须在数据目录之外：
/// 导入会让数据目录整体改名让位，解压句柄若在其内会导致改名失败）。
/// 结束/失败后由调用侧清理。
pub(crate) fn create_temp_dir(prefix: &str) -> Result<PathBuf, String> {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = std::env::temp_dir().join(format!("{prefix}-{nanos}"));
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建临时目录失败: {e}"))?;
    Ok(dir)
}

pub(crate) fn io_err(e: std::io::Error) -> String {
    format!("写入文件失败: {e}")
}

/// 按归一化路径查找条目的原始名（Windows Compress-Archive 可能用反斜杠存储条目名）。
pub(crate) fn raw_name_of(
    archive: &mut ZipArchive<std::fs::File>,
    normalized: &str,
) -> Option<String> {
    for i in 0..archive.len() {
        if let Ok(f) = archive.by_index(i) {
            if f.name().replace('\\', "/") == normalized {
                return Some(f.name().to_string());
            }
        }
    }
    None
}

/// 按归一化路径读取一个条目为字符串。
pub(crate) fn read_entry_to_string(
    archive: &mut ZipArchive<std::fs::File>,
    normalized: &str,
) -> Result<String, String> {
    let raw = raw_name_of(archive, normalized).ok_or_else(|| format!("备份缺少 {normalized}"))?;
    let mut entry = archive
        .by_name(&raw)
        .map_err(|e| format!("读取 {normalized} 失败: {e}"))?;
    let mut buf = String::new();
    entry
        .read_to_string(&mut buf)
        .map_err(|e| format!("读取 {normalized} 失败: {e}"))?;
    Ok(buf)
}

/// 把包内指定条目解压为单个文件。
pub(crate) fn extract_entry(
    archive: &mut ZipArchive<std::fs::File>,
    normalized: &str,
    dest: &Path,
) -> Result<(), String> {
    let raw = raw_name_of(archive, normalized).ok_or_else(|| format!("备份缺少 {normalized}"))?;
    let mut entry = archive
        .by_name(&raw)
        .map_err(|e| format!("读取 {normalized} 失败: {e}"))?;
    let mut out = std::fs::File::create(dest).map_err(|e| format!("创建临时文件失败: {e}"))?;
    std::io::copy(&mut entry, &mut out).map_err(|e| format!("解压 {normalized} 失败: {e}"))?;
    Ok(())
}

/// 在包内定位 manifest.json，返回其目录前缀（"" 或 "dir/"）。
/// 兼容 Unix `zip -r` 打包时多出的一层临时目录包裹。
pub(crate) fn locate_root(archive: &mut ZipArchive<std::fs::File>) -> Result<String, String> {
    let mut normalized: Vec<String> = Vec::with_capacity(archive.len());
    for i in 0..archive.len() {
        let f = archive
            .by_index(i)
            .map_err(|e| format!("读取 ZIP 条目失败: {e}"))?;
        normalized.push(f.name().replace('\\', "/"));
    }
    let prefixes: Vec<&str> = normalized
        .iter()
        .filter_map(|n| n.strip_suffix(MANIFEST_ENTRY))
        .filter(|p| p.is_empty() || (p.ends_with('/') && !p[..p.len() - 1].contains('/')))
        .collect();
    match prefixes.len() {
        1 => Ok(prefixes[0].to_string()),
        0 => Err("无效的备份文件：缺少 manifest.json".into()),
        _ => Err("无效的备份文件：manifest.json 不唯一".into()),
    }
}

/// 统计图像目录下的文件条目数（用于进度）。
pub(crate) fn count_image_entries(
    archive: &mut ZipArchive<std::fs::File>,
    image_prefix: &str,
) -> usize {
    let mut n = 0usize;
    for i in 0..archive.len() {
        if let Ok(f) = archive.by_index(i) {
            if f.is_dir() {
                continue;
            }
            let norm = f.name().replace('\\', "/");
            if norm.starts_with(image_prefix) && norm.len() > image_prefix.len() {
                n += 1;
            }
        }
    }
    n
}

/// 递归收集 dir 下所有文件为（绝对路径, 相对 base 的正斜杠路径）。
pub(crate) fn collect_files(
    dir: &Path,
    base: &Path,
    out: &mut Vec<(PathBuf, String)>,
) -> std::io::Result<()> {
    if !dir.exists() {
        return Ok(());
    }
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_files(&path, base, out)?;
        } else {
            let rel = path
                .strip_prefix(base)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            out.push((path, rel));
        }
    }
    Ok(())
}

/// 递归复制目录（源不存在视为空返回 0），每复制一个文件回调一次（已完成数/总数/相对路径）。
pub(crate) fn copy_dir_with_progress<F>(src: &Path, dst: &Path, on_file: F) -> Result<usize, String>
where
    F: Fn(usize, usize, &str),
{
    let mut files = Vec::new();
    collect_files(src, src, &mut files).map_err(|e| format!("扫描目录失败: {e}"))?;
    let total = files.len();
    for (i, (from, rel)) in files.iter().enumerate() {
        let to = dst.join(rel);
        if let Some(parent) = to.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("创建目录失败: {e}"))?;
        }
        std::fs::copy(from, &to).map_err(|e| format!("复制 {rel} 失败: {e}"))?;
        on_file(i + 1, total, rel);
    }
    Ok(total)
}

/// 在指定路径打开应用数据库（含建表/迁移），返回裸连接供装入 BkDb。
pub(crate) fn open_app_db(path: &Path) -> Result<Connection, String> {
    db::init(path.to_path_buf())
        .map_err(|e| format!("初始化数据库失败: {e}"))?
        .0
        .into_inner()
        .map_err(|_| "数据库句柄已损坏".to_string())
}

/// 探测备份包来源：读 manifest appName，归一化为 `"paim"` / `"pm"`。
/// 仅识别来源；版本与完整性校验仍由对应 service 的 inspect/import 负责。
pub(crate) fn detect_app(zip_path: &str) -> Result<String, String> {
    let file = std::fs::File::open(zip_path).map_err(|e| format!("无法打开备份文件: {e}"))?;
    let mut archive = ZipArchive::new(file).map_err(|e| format!("备份文件不是有效的 ZIP: {e}"))?;
    let root = locate_root(&mut archive)?;
    let content = read_entry_to_string(&mut archive, &format!("{root}{MANIFEST_ENTRY}"))?;
    #[derive(Deserialize)]
    struct AppName {
        #[serde(rename = "appName")]
        app_name: String,
    }
    let m: AppName =
        serde_json::from_str(&content).map_err(|_| "manifest.json 格式无效".to_string())?;
    match m.app_name.as_str() {
        "paim" => Ok("paim".into()),
        "prompt-manager" => Ok("pm".into()),
        other => Err(format!("无法识别的备份来源: {other}")),
    }
}
