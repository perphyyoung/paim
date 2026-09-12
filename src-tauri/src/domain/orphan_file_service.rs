//! 孤儿文件扫描 + 导出删除。
//!
//! 孤儿文件 = 磁盘存在但 DB 完全无对应记录（deleted_at 不影响——DB 有记录就不算孤儿）。
//! 两类独立扫描：原图像（images/ 目录下非数据库 relative_path 的文件）
//! 和缩略图（thumbnails/ 目录下非数据库 thumbnail_path 的文件）。
//! 清理策略：原图像复制到用户选定的导出目录后删源；缩略图直接删除不导出。
//!
//! 路径基准对齐：DB 的 relative_path / thumbnail_path 都是相对于 data_dir
//! （见 image_service::create_image 存的 `images/{yyyymm}/{stored_name}`），
//! 磁盘遍历必须也返回相对于 data_dir 的路径才能做差集。

use rusqlite::Connection;
use std::fs;
use std::path::{Path, PathBuf};

/// 扫描结果（不含大小统计——按用户要求只计数）
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
pub struct OrphanScanResult {
    pub orphan_image_count: i64,
    pub orphan_thumbnail_count: i64,
}

/// 导出删除结果
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
pub struct OrphanExportResult {
    /// 导出的孤儿原图像数
    pub exported: i64,
    /// 删除的孤儿文件数（原图像 + 缩略图）
    pub deleted: i64,
    /// 失败数（复制或删除出错，单文件失败不中断）
    pub failed: i64,
    /// 导出目录（空串表示无孤儿或无导出发生）
    pub export_path: String,
}

/// 查出 DB 中全部 `relative_path` 和 `thumbnail_path`（含回收站）。
/// 路径相对于 data_dir，用于与磁盘遍历结果做差集。
fn db_path_sets(conn: &Connection) -> rusqlite::Result<(Vec<String>, Vec<String>)> {
    let mut stmt = conn.prepare(
        "SELECT relative_path, thumbnail_path FROM images WHERE relative_path IS NOT NULL",
    )?;
    let rows: Vec<(Option<String>, Option<String>)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<Vec<_>, _>>()?;
    let mut image_paths = Vec::new();
    let mut thumb_paths = Vec::new();
    for (rel, thumb) in rows {
        if let Some(p) = rel {
            image_paths.push(p);
        }
        if let Some(p) = thumb {
            thumb_paths.push(p);
        }
    }
    Ok((image_paths, thumb_paths))
}

/// 递归收集 `dir` 下所有文件，返回 `(相对 base_dir 的正斜杠字符串, 绝对路径)` 二元组。
/// base_dir 通常就是数据目录——DB 的 relative_path / thumbnail_path 也相对于它。
///
/// 路径分隔符统一转 `/`（对齐 pm 的 `.replace(/\\/g, "/")`）。Windows 下 PathBuf
/// 用 `\` 分隔，DB 存的是 `/`，不转的话差集永远匹配不上——所有文件都会被当成孤儿。
/// 目录不存在返回空列表（不视为错误）。
fn walk_files(dir: &Path, base_dir: &Path) -> Vec<(String, PathBuf)> {
    let mut out = Vec::new();
    let Ok(rd) = fs::read_dir(dir) else {
        return out;
    };
    for entry in rd.flatten() {
        let path = entry.path();
        if path.is_dir() {
            out.extend(walk_files(&path, base_dir));
        } else if let Ok(rel) = path.strip_prefix(base_dir) {
            let rel_str = rel.to_string_lossy().replace('\\', "/");
            out.push((rel_str, path));
        }
    }
    out
}

/// 扫描孤儿文件（阻塞，应在 spawn_blocking 内调用）
pub fn scan_orphan_files(conn: &Connection, data_dir: &Path) -> rusqlite::Result<OrphanScanResult> {
    let (db_images, db_thumbs) = db_path_sets(conn)?;
    let db_images: std::collections::HashSet<String> = db_images.into_iter().collect();
    let db_thumbs: std::collections::HashSet<String> = db_thumbs.into_iter().collect();

    let disk_images = walk_files(&data_dir.join("images"), data_dir);
    let disk_thumbs = walk_files(&data_dir.join("thumbnails"), data_dir);

    let orphan_image_count = disk_images
        .iter()
        .filter(|(rel, _)| !db_images.contains(rel))
        .count() as i64;
    let orphan_thumbnail_count = disk_thumbs
        .iter()
        .filter(|(rel, _)| !db_thumbs.contains(rel))
        .count() as i64;

    Ok(OrphanScanResult {
        orphan_image_count,
        orphan_thumbnail_count,
    })
}

/// 扫描孤儿文件并返回（相对路径, 绝对路径）清单，供导出删除用。
fn scan_detailed(
    conn: &Connection,
    data_dir: &Path,
) -> rusqlite::Result<(Vec<PathBuf>, Vec<PathBuf>)> {
    let (db_images, db_thumbs) = db_path_sets(conn)?;
    let db_images: std::collections::HashSet<String> = db_images.into_iter().collect();
    let db_thumbs: std::collections::HashSet<String> = db_thumbs.into_iter().collect();

    let disk_images = walk_files(&data_dir.join("images"), data_dir);
    let disk_thumbs = walk_files(&data_dir.join("thumbnails"), data_dir);

    let orphan_images: Vec<PathBuf> = disk_images
        .into_iter()
        .filter(|(rel, _)| !db_images.contains(rel))
        .map(|(_, abs)| abs)
        .collect();
    let orphan_thumbs: Vec<PathBuf> = disk_thumbs
        .into_iter()
        .filter(|(rel, _)| !db_thumbs.contains(rel))
        .map(|(_, abs)| abs)
        .collect();

    Ok((orphan_images, orphan_thumbs))
}

/// 导出并删除孤儿文件（阻塞，应在 spawn_blocking 内调用）。
/// 原图像复制到 `orphan_export_dir` 后删源；缩略图直接删除。
/// 单文件失败计入 failed，不中断。`orphan_export_dir` 必须由调用方创建并存在。
pub fn export_orphan_files(
    conn: &Connection,
    data_dir: &Path,
    orphan_export_dir: &Path,
) -> rusqlite::Result<OrphanExportResult> {
    let (orphan_images, orphan_thumbs) = scan_detailed(conn, data_dir)?;
    if orphan_images.is_empty() && orphan_thumbs.is_empty() {
        return Ok(OrphanExportResult {
            exported: 0,
            deleted: 0,
            failed: 0,
            export_path: String::new(),
        });
    }

    let mut exported = 0i64;
    let mut deleted = 0i64;
    let mut failed = 0i64;

    // 原图像：复制到导出目录后删除源文件
    for src in &orphan_images {
        let name = src
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "unknown".to_string());
        let dest = orphan_export_dir.join(&name);
        // 同名冲突时追加序号
        let dest = if dest.exists() {
            let stem = src
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            let ext = src
                .extension()
                .map(|s| format!(".{}", s.to_string_lossy()))
                .unwrap_or_default();
            let mut i = 1u32;
            loop {
                let alt = orphan_export_dir.join(format!("{stem}_{i}{ext}"));
                if !alt.exists() {
                    break alt;
                }
                i += 1;
            }
        } else {
            dest
        };
        match fs::copy(src, &dest) {
            Ok(_) => {
                exported += 1;
                if fs::remove_file(src).is_ok() {
                    deleted += 1;
                } else {
                    failed += 1;
                }
            }
            Err(_) => {
                failed += 1;
            }
        }
    }

    // 缩略图直接删除
    for p in &orphan_thumbs {
        if fs::remove_file(p).is_ok() {
            deleted += 1;
        } else {
            failed += 1;
        }
    }

    Ok(OrphanExportResult {
        exported,
        deleted,
        failed,
        export_path: orphan_export_dir.to_string_lossy().into_owned(),
    })
}
