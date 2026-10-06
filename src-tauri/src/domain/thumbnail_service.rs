//! 缩略图服务：200×200 居中裁剪缩略图的生成与全量重建（补缺失）。
//! 供两个场景共用：pm 备份导入后的缩略图重建、设置页「重建缩略图」。
//! 重建语义与 pm 一致：扫描所有图像记录，缩略图文件已存在的直接复用，
//! 只对丢失的重新生成；失败的单张计数，不中断整体、不改动其已有路径。

use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use crate::domain::image_ops::{make_center_thumb, open_image};

/// 全量重建结果摘要。success 包含「已存在跳过」与「新生成」两类
/// （与 pm 的 regenerated 计数口径一致）。
#[derive(Debug, Serialize, specta::Type)]
pub struct ThumbnailRebuildSummary {
    pub total: usize,
    pub success: usize,
    pub failed: usize,
}

/// 重建进度推送载荷（事件名固定为 thumbnail-rebuild-progress）。
#[derive(Debug, Serialize, Deserialize, Clone, specta::Type, tauri_specta::Event)]
#[tauri_specta(event_name = "thumbnail-rebuild-progress")]
pub struct ThumbnailRebuildProgress {
    pub current: usize,
    pub total: usize,
    pub file_name: String,
}

/// 懒自愈结果：fixed 为已补齐缩略图的记录（含新回写路径），
/// missing 为无法修复的 id（记录不存在 / 原图缺失 / 生成失败）。
#[derive(Debug, Serialize, specta::Type)]
pub struct ThumbnailEnsureResult {
    pub fixed: Vec<ThumbnailEnsureFixed>,
    pub missing: Vec<String>,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct ThumbnailEnsureFixed {
    pub id: String,
    pub thumbnail_path: String,
}

/// 单图缩略图生成：读原图 → 解码 → 200×200 居中裁剪 → 存
/// thumbnails/{YYYYMM}/thumb_{stored_name 词干}.jpg，
/// 返回要写回 images.thumbnail_path 的相对路径；失败原因以 Err 返回。
/// 缩略图文件已存在时直接返回现路径（与 pm 的 generateThumbnail 一致）。
pub fn build_thumbnail(
    data_dir: &Path,
    thumbs_root: &Path,
    rel: &str,
    image_id: Option<&str>,
    file_name: Option<&str>,
) -> Result<String, String> {
    // 年月子目录取自 relative_path 第二段（images/202608/x.png → 202608）
    let rel_path = Path::new(rel);
    let month = rel_path
        .components()
        .nth(1)
        .and_then(|c| c.as_os_str().to_str())
        .unwrap_or("");
    let thumb_dir = if month.is_empty() {
        thumbs_root.to_path_buf()
    } else {
        thumbs_root.join(month)
    };
    let thumb_rel_prefix = if month.is_empty() {
        "thumbnails".to_string()
    } else {
        format!("thumbnails/{month}")
    };

    // relative_path 以 stored_name 结尾，取词干即 pm 的缩略图命名
    let stem = rel_path
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| format!("无效的图像路径: {rel}"))?;
    let name = format!("thumb_{stem}.jpg");

    let thumb_path = thumb_dir.join(&name);
    if thumb_path.is_file() {
        return Ok(format!("{thumb_rel_prefix}/{name}"));
    }

    let full_path = crate::infra::db::data_path(data_dir, rel);
    if !full_path.exists() {
        crate::log_warn!(
            "image_missing: id={} file_name={} caller=build_thumbnail",
            image_id.unwrap_or(""),
            file_name.unwrap_or("")
        );
    }
    let img = open_image(&full_path).map_err(|e| format!("读取图像失败: {e}"))?;
    let thumb = make_center_thumb(&img).map_err(|e| format!("生成缩略图失败: {e}"))?;
    std::fs::create_dir_all(&thumb_dir).map_err(io_err)?;
    // 编码用 jpeg-encoder（SIMD，image 自带编码器无 SIMD），质量 80 与 pm 一致
    let rgb = thumb.to_rgb8();
    let mut file = std::fs::File::create(&thumb_path).map_err(io_err)?;
    jpeg_encoder::Encoder::new(&mut file, 80)
        .encode(
            rgb.as_raw(),
            rgb.width() as u16,
            rgb.height() as u16,
            jpeg_encoder::ColorType::Rgb,
        )
        .map_err(|e| format!("保存缩略图失败: {e}"))?;
    Ok(format!("{thumb_rel_prefix}/{name}"))
}

/// 校验 / 重建所需的单个目标：**短锁阶段**取出的库内数据，不含任何磁盘结论。
#[derive(Debug, Clone)]
pub struct ThumbTarget {
    pub id: String,
    /// 相对数据目录的原图路径
    pub rel: String,
    pub file_name: String,
    /// 库里的 thumbnail_path（None = 尚未生成）
    pub current: Option<String>,
}

/// 短锁阶段：按 id 取校验所需的库内数据；记录不存在的 id 单独回报（调用方计入 missing）。
/// 只查库、不碰磁盘——「先 stat 再决定生不生成」属于无锁阶段。
pub fn targets_by_ids(
    conn: &Connection,
    ids: &[String],
) -> Result<(Vec<ThumbTarget>, Vec<String>), String> {
    let mut targets = Vec::with_capacity(ids.len());
    let mut unknown = Vec::new();
    for id in ids {
        let row: Option<(String, String, Option<String>)> = conn
            .query_row(
                "SELECT relative_path, file_name, thumbnail_path FROM images WHERE id = ?1",
                rusqlite::params![id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()
            .map_err(|e| format!("读取图像记录失败: {e}"))?;
        match row {
            Some((rel, file_name, current)) => targets.push(ThumbTarget {
                id: id.clone(),
                rel,
                file_name,
                current,
            }),
            None => unknown.push(id.clone()),
        }
    }
    Ok((targets, unknown))
}

/// 短锁阶段：取全部图像记录（全量重建用）。收集完即可放锁，生成过程不需要连接。
pub fn all_targets(conn: &Connection) -> Result<Vec<ThumbTarget>, String> {
    let mut stmt = conn
        .prepare("SELECT id, relative_path, file_name, thumbnail_path FROM images")
        .map_err(|e| format!("读取图像记录失败: {e}"))?;
    let rows = stmt
        .query_map([], |r| {
            Ok(ThumbTarget {
                id: r.get(0)?,
                rel: r.get(1)?,
                file_name: r.get(2)?,
                current: r.get(3)?,
            })
        })
        .map_err(|e| format!("读取图像记录失败: {e}"))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("读取图像记录失败: {e}"))
}

/// 无锁阶段的逐项结果。
pub struct ThumbBuildOutcome {
    /// 需要回写库的项（新生成，或库内路径缺失但磁盘上已有派生路径）
    pub fixed: Vec<ThumbnailEnsureFixed>,
    /// 无法修复的 id（原图缺失 / 解码失败）
    pub missing: Vec<String>,
    /// 成功数（已有文件跳过 + 新生成）——全量重建的 success 口径与 pm 的 regenerated 一致
    pub success: usize,
}

/// **无锁阶段**：逐个校验目标，缺图才生成（健康项只 stat，不生成、不回写）。
/// `concurrency` 由调用方按场景给：全量重建可满核（代价是峰值内存，每线程一张解码位图，
/// 见 docs/导入优化.md）；懒自愈传 1（一屏项数，顺序足够且不抬内存峰值）。
/// 进度回调参数：(已完成, 总数, 文件名)。
pub fn build_missing<F>(
    data_dir: &Path,
    thumbs_root: &Path,
    targets: &[ThumbTarget],
    concurrency: usize,
    on_progress: F,
) -> ThumbBuildOutcome
where
    F: Fn(usize, usize, &str) + Sync,
{
    let _ = std::fs::create_dir_all(thumbs_root);
    let total = targets.len();
    let next = AtomicUsize::new(0);
    let completed = AtomicUsize::new(0);
    let success = AtomicUsize::new(0);
    // (id, 生成结果, file_name)：失败项也要带回 file_name 便于日志定位
    let results: Mutex<Vec<(String, Result<String, String>, String)>> =
        Mutex::new(Vec::with_capacity(total));
    let workers = total.min(concurrency.max(1));
    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| loop {
                let idx = next.fetch_add(1, Ordering::Relaxed);
                if idx >= total {
                    break;
                }
                let target = &targets[idx];
                // 健康项：库里路径非空、且文件还在 → 只 stat 后跳过
                if let Some(cur) = &target.current {
                    if crate::infra::db::data_path(data_dir, cur).is_file() {
                        success.fetch_add(1, Ordering::Relaxed);
                        let done = completed.fetch_add(1, Ordering::Relaxed) + 1;
                        on_progress(done, total, &target.file_name);
                        continue;
                    }
                }
                let outcome = build_thumbnail(
                    data_dir,
                    thumbs_root,
                    &target.rel,
                    Some(&target.id),
                    Some(&target.file_name),
                );
                if outcome.is_ok() {
                    success.fetch_add(1, Ordering::Relaxed);
                }
                results.lock().unwrap().push((
                    target.id.clone(),
                    outcome,
                    target.file_name.clone(),
                ));
                let done = completed.fetch_add(1, Ordering::Relaxed) + 1;
                on_progress(done, total, &target.file_name);
            });
        }
    });

    let outcomes = results
        .into_inner()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut fixed = Vec::new();
    let mut missing = Vec::new();
    for (id, outcome, file_name) in outcomes {
        match outcome {
            Ok(thumbnail_path) => fixed.push(ThumbnailEnsureFixed { id, thumbnail_path }),
            Err(msg) => {
                crate::log_warn!("缩略图缺失且无法生成 id={id} file_name={file_name}: {msg}");
                missing.push(id);
            }
        }
    }
    // 并发完成顺序不定：按 id 排序，让结果可对账（也便于单测断言）
    fixed.sort_by(|a, b| a.id.cmp(&b.id));
    missing.sort();
    ThumbBuildOutcome {
        fixed,
        missing,
        success: success.load(Ordering::Relaxed),
    }
}

/// 短锁阶段：批量回写 thumbnail_path（单事务；只写成功项，失败项保留原路径）。
pub fn write_paths(conn: &Connection, fixed: &[ThumbnailEnsureFixed]) -> Result<(), String> {
    if fixed.is_empty() {
        return Ok(());
    }
    conn.execute_batch("BEGIN IMMEDIATE;")
        .map_err(|e| format!("开启缩略图回写事务失败: {e}"))?;
    for item in fixed {
        if let Err(e) = conn.execute(
            "UPDATE images SET thumbnail_path = ?1 WHERE id = ?2",
            rusqlite::params![item.thumbnail_path, item.id],
        ) {
            let _ = conn.execute_batch("ROLLBACK;");
            return Err(format!("更新缩略图路径失败: {e}"));
        }
    }
    conn.execute_batch("COMMIT;")
        .map_err(|e| format!("提交缩略图回写事务失败: {e}"))?;
    Ok(())
}

fn io_err(e: std::io::Error) -> String {
    e.to_string()
}

#[cfg(test)]
#[path = "thumbnail_service.test.rs"]
mod tests;
