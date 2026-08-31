//! 缩略图服务：200×200 居中裁剪缩略图的生成与全量重建（补缺失）。
//! 供两个场景共用：pm 备份导入后的缩略图重建、设置页「重建缩略图」。
//! 重建语义与 pm 一致：扫描所有图像记录，缩略图文件已存在的直接复用，
//! 只对丢失的重新生成；失败的单张计数，不中断整体、不改动其已有路径。

use rusqlite::Connection;
use serde::Serialize;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

/// 重建进度推送事件名（前端 listen 用）。
pub const PROGRESS_EVENT: &str = "thumbnail-rebuild-progress";

/// 全量重建结果摘要。success 包含「已存在跳过」与「新生成」两类
/// （与 pm 的 regenerated 计数口径一致）。
#[derive(Debug, Serialize)]
pub struct RebuildSummary {
    pub total: usize,
    pub success: usize,
    pub failed: usize,
}

/// 重建进度推送载荷。
#[derive(Debug, Serialize, Clone)]
pub struct RebuildProgress {
    pub current: usize,
    pub total: usize,
    pub file_name: String,
}

/// 单图缩略图生成：读原图 → 解码 → 200×200 居中裁剪 → 存
/// thumbnails/{YYYYMM}/thumb_{stored_name 词干}.jpg，
/// 返回要写回 images.thumbnail_path 的相对路径；失败原因以 Err 返回。
/// 缩略图文件已存在时直接返回现路径（与 pm 的 generateThumbnail 一致）。
pub fn build_thumbnail(data_dir: &Path, thumbs_root: &Path, rel: &str) -> Result<String, String> {
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

    let img = image::open(data_dir.join(rel)).map_err(|e| format!("读取图像失败: {e}"))?;
    let thumb = crate::features::image_service::make_center_thumb(&img)
        .map_err(|e| format!("生成缩略图失败: {e}"))?;
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

/// 全量重建缩略图：扫描全部图像记录，补齐丢失的缩略图文件并批量回写
/// thumbnail_path（paim 布局：thumbnails/{YYYYMM}/thumb_{stored_name 词干}.jpg）。
/// 失败的记录保留原有 thumbnail_path 并计数。进度回调参数：(已完成, 总数, 文件名)。
/// 参照 pm 的并发模型：多工作线程并发生成，结束后单事务批量回写。
/// 线程数取系统可用并行度（满核跑，不做写死上限），代价是峰值内存
/// （每线程持有一张解码位图，详见 导入优化.md）。
pub fn rebuild_all<F>(
    data_dir: &Path,
    thumbs_root: &Path,
    conn: &Connection,
    on_progress: F,
) -> Result<RebuildSummary, String>
where
    F: Fn(usize, usize, &str) + Sync,
{
    std::fs::create_dir_all(thumbs_root).map_err(io_err)?;

    // 先收集再处理，避免边查询边更新同一张表
    let rows: Vec<(String, String, String)> = {
        let mut stmt = conn
            .prepare("SELECT id, relative_path, file_name FROM images")
            .map_err(|e| format!("读取图像记录失败: {e}"))?;
        let mapped = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .map_err(|e| format!("读取图像记录失败: {e}"))?;
        mapped
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("读取图像记录失败: {e}"))?
    };
    let total = rows.len();
    if total == 0 {
        return Ok(RebuildSummary { total: 0, success: 0, failed: 0 });
    }

    // 并发生成：游标领任务，结果集中收集；进度计数共享，由完成线程直接推送
    let next = AtomicUsize::new(0);
    let completed = AtomicUsize::new(0);
    let results: Mutex<Vec<(String, Result<String, String>)>> = Mutex::new(Vec::with_capacity(total));
    let workers = total.min(
        std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4),
    );
    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| loop {
                let idx = next.fetch_add(1, Ordering::Relaxed);
                if idx >= rows.len() {
                    break;
                }
                let (id, rel, file_name) = &rows[idx];
                let outcome = build_thumbnail(data_dir, thumbs_root, rel);
                results.lock().unwrap().push((id.clone(), outcome));
                let done = completed.fetch_add(1, Ordering::Relaxed) + 1;
                on_progress(done, total, file_name);
            });
        }
    });

    // 单事务批量回写 thumbnail_path（仅成功项；失败项保留原路径并计数）
    let outcomes = results
        .into_inner()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    conn.execute_batch("BEGIN IMMEDIATE;")
        .map_err(|e| format!("开启缩略图回写事务失败: {e}"))?;
    let mut success = 0usize;
    let mut failed = 0usize;
    for (id, outcome) in outcomes {
        match outcome {
            Ok(thumb_rel) => {
                if let Err(e) = conn.execute(
                    "UPDATE images SET thumbnail_path = ?1 WHERE id = ?2",
                    rusqlite::params![thumb_rel, id],
                ) {
                    let _ = conn.execute_batch("ROLLBACK;");
                    return Err(format!("更新缩略图路径失败: {e}"));
                }
                success += 1;
            }
            Err(msg) => {
                failed += 1;
                log::warn!("缩略图重建失败 id={id}: {msg}");
            }
        }
    }
    conn.execute_batch("COMMIT;")
        .map_err(|e| format!("提交缩略图回写事务失败: {e}"))?;
    Ok(RebuildSummary { total, success, failed })
}

fn io_err(e: std::io::Error) -> String {
    e.to_string()
}

#[cfg(test)]
#[path = "thumbnail_service.test.rs"]
mod tests;
