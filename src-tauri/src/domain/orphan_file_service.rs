//! 数据完整性检查：孤儿文件（磁盘有 DB 无）+ 孤儿记录（DB 有磁盘无原图）。
//!
//! 路径基准对齐：DB 的 relative_path / thumbnail_path 都是相对于 data_dir
//! （见 image_service::create_image 存的 `images/{yyyymm}/{stored_name}`），
//! 磁盘遍历必须也返回相对于 data_dir 的路径才能做差集。
//!
//! 分隔符统一转 `/`（对齐 pm 的 `.replace(/\\/g, "/")`）。Windows 下 PathBuf
//! 用 `\` 分隔，DB 存的是 `/`，不转的话差集永远不匹配。
//!
//! 缩略图缺失不算孤儿记录——thumbnail_service 有懒自愈机制按需生成。

use rusqlite::Connection;
use std::fs;
use std::path::{Path, PathBuf};

/// 孤儿记录条目（DB 有但原图磁盘文件不存在）
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
pub struct OrphanRecordItem {
    pub id: String,
    /// 用户可见的文件名
    pub file_name: String,
    /// 落盘的存储名
    pub stored_name: String,
    /// DB 中的相对路径（相对于 data_dir）
    pub relative_path: String,
}

/// 完整性检查结果
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
pub struct IntegrityCheckResult {
    /// 孤儿原图像文件数（磁盘有、DB 无 relative_path）
    pub orphan_image_count: i64,
    /// 孤儿缩略图文件数（磁盘有、DB 无 thumbnail_path）
    pub orphan_thumbnail_count: i64,
    /// 孤儿记录（DB 有 relative_path 但磁盘原图不存在，不含缩略图）
    pub orphan_records: Vec<OrphanRecordItem>,
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

/// 查出 DB 中全部 `relative_path` 和 `thumbnail_path`（含回收站），路径相对于 data_dir。
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

/// 查出 DB 中全部 id + file_name + stored_name + relative_path（含回收站），
/// 用于反向检查孤儿记录（原图缺失）。
fn db_all_images(conn: &Connection) -> rusqlite::Result<Vec<OrphanRecordItem>> {
    let mut stmt = conn.prepare(
        "SELECT id, file_name, stored_name, relative_path FROM images WHERE relative_path IS NOT NULL",
    )?;
    let rows = stmt.query_map([], |r| {
        let id: String = r.get(0)?;
        let file_name: String = r.get(1)?;
        let stored_name: String = r.get(2)?;
        let relative_path: String = r.get(3)?;
        Ok((id, file_name, stored_name, relative_path))
    })?;
    rows.collect::<Result<Vec<_>, _>>().map(|v| {
        v.into_iter()
            .map(
                |(id, file_name, stored_name, relative_path)| OrphanRecordItem {
                    id,
                    file_name,
                    stored_name,
                    relative_path,
                },
            )
            .collect()
    })
}

/// 递归收集 `dir` 下所有文件，返回 `(相对 base_dir 的正斜杠字符串, 绝对路径)` 二元组。
/// base_dir 通常就是数据目录。目录不存在返回空列表。
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

/// 数据完整性检查（阻塞，应在 spawn_blocking 内调用）
pub fn scan_integrity(
    conn: &Connection,
    data_dir: &Path,
) -> rusqlite::Result<IntegrityCheckResult> {
    let (db_images, db_thumbs) = db_path_sets(conn)?;
    let db_images_set: std::collections::HashSet<String> = db_images.into_iter().collect();
    let db_thumbs_set: std::collections::HashSet<String> = db_thumbs.into_iter().collect();

    let disk_images = walk_files(&data_dir.join("images"), data_dir);
    let disk_thumbs = walk_files(&data_dir.join("thumbnails"), data_dir);
    let disk_images_set: std::collections::HashSet<&String> =
        disk_images.iter().map(|(r, _)| r).collect();

    // 正向：磁盘有、DB 无 → 孤儿文件
    let orphan_image_count = disk_images
        .iter()
        .filter(|(rel, _)| !db_images_set.contains(rel))
        .count() as i64;
    let orphan_thumbnail_count = disk_thumbs
        .iter()
        .filter(|(rel, _)| !db_thumbs_set.contains(rel))
        .count() as i64;

    // 反向：DB 有 relative_path、磁盘原图不存在 → 孤儿记录
    let all_db_images = db_all_images(conn)?;
    let orphan_records: Vec<OrphanRecordItem> = all_db_images
        .into_iter()
        .filter(|item| !disk_images_set.contains(&item.relative_path))
        .collect();

    Ok(IntegrityCheckResult {
        orphan_image_count,
        orphan_thumbnail_count,
        orphan_records,
    })
}

/// 扫描孤儿文件的磁盘路径清单，供导出删除用。
fn scan_orphan_paths(
    conn: &Connection,
    data_dir: &Path,
) -> rusqlite::Result<(Vec<PathBuf>, Vec<PathBuf>)> {
    let (db_images, db_thumbs) = db_path_sets(conn)?;
    let db_images_set: std::collections::HashSet<String> = db_images.into_iter().collect();
    let db_thumbs_set: std::collections::HashSet<String> = db_thumbs.into_iter().collect();

    let disk_images = walk_files(&data_dir.join("images"), data_dir);
    let disk_thumbs = walk_files(&data_dir.join("thumbnails"), data_dir);

    let orphan_images: Vec<PathBuf> = disk_images
        .into_iter()
        .filter(|(rel, _)| !db_images_set.contains(rel))
        .map(|(_, abs)| abs)
        .collect();
    let orphan_thumbs: Vec<PathBuf> = disk_thumbs
        .into_iter()
        .filter(|(rel, _)| !db_thumbs_set.contains(rel))
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
    let (orphan_images, orphan_thumbs) = scan_orphan_paths(conn, data_dir)?;
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

    for src in &orphan_images {
        let name = src
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "unknown".to_string());
        let dest = orphan_export_dir.join(&name);
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
