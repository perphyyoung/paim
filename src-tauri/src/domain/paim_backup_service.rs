//! paim 自有全量备份导出/导入服务。
//! 备份包结构（本服务导出）：
//!   manifest.json + database/paim.db + files/images/**（缩略图不导出，导入端重建）。
//! 导出：统计 → manifest → `VACUUM INTO` 生成库快照（原子、自动合并 WAL，不断库）
//!   → 复制 images → ZipWriter 压缩，全程在临时目录暂存。
//! 导入语义与 pm 一致：整体替换当前数据——整目录让位（同级 `paim-data_{时间戳}`）
//!   + 直接换库文件（paim 自有 schema 无需逐表转换，重开库时 `db::init` 自动迁移），
//!   再重建缩略图；失败自动回滚（删半成品、备份目录归位、重开原库）。

use rusqlite::Connection;
use serde::Deserialize;
use std::path::Path;
use zip::ZipArchive;

use crate::domain::backup_common::{
    collect_files, copy_dir_with_progress, count_image_entries, create_temp_dir, extract_entry,
    io_err, is_safe_rel_path, locate_root, open_app_db, read_entry_to_string, BackupExportSummary,
    BackupImportSummary, BackupInfo, BackupProgress, MANIFEST_ENTRY,
};
use crate::infra::db::{self, BkDb};
use crate::{log_error, log_info};

/// 当前支持的数据格式版本；导入接受 ≤ 该版本（旧备份重开库时由迁移升级）。
const CURRENT_DATA_VERSION: i64 = 1;

/// paim 备份包内的固定布局。
const DB_ENTRY: &str = "database/paim.db";
const IMAGES_ENTRY_PREFIX: &str = "files/images/";

/// manifest.json 导出所需字段（appName/dataVersion 与 pm 同名字段语义一致）。
#[derive(Debug, Deserialize)]
struct BackupManifest {
    #[serde(rename = "appName")]
    app_name: String,
    #[serde(rename = "exportedAt", default)]
    exported_at: String,
    #[serde(rename = "dataVersion", default)]
    data_version: Option<i64>,
}

/// 概览/摘要/进度事件复用 backup_common 的共享类型（pm/paim 同构）；导出走 BackupExportSummary。

fn count(conn: &Connection, sql: &str) -> Result<i64, String> {
    conn.query_row(sql, [], |r| r.get(0))
        .map_err(|e| format!("统计数据失败: {e}"))
}

/// 解析备份包，返回内容概览（不改动任何本地数据）。
pub fn inspect(zip_path: &str) -> Result<BackupInfo, String> {
    let file = std::fs::File::open(zip_path).map_err(|e| format!("无法打开备份文件: {e}"))?;
    let mut archive = ZipArchive::new(file).map_err(|e| format!("备份文件不是有效的 ZIP: {e}"))?;

    let root = locate_root(&mut archive)?;
    let manifest = read_manifest(&mut archive, &root)?;
    validate_manifest(&manifest)?;

    let tmp = create_temp_dir("paim-backup-inspect")?;
    let result = (|| {
        let db_file = tmp.join("paim.db");
        extract_entry(&mut archive, &format!("{root}{DB_ENTRY}"), &db_file)?;
        let conn =
            Connection::open_with_flags(&db_file, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
                .map_err(|e| format!("打开备份数据库失败: {e}"))?;
        Ok(BackupInfo {
            exported_at: manifest.exported_at.clone(),
            prompt_count: count(&conn, "SELECT COUNT(*) FROM prompts")?,
            image_count: count(&conn, "SELECT COUNT(*) FROM images")?,
            trashed_prompt_count: count(
                &conn,
                "SELECT COUNT(*) FROM prompts WHERE is_deleted = 1",
            )?,
            trashed_image_count: count(&conn, "SELECT COUNT(*) FROM images WHERE is_deleted = 1")?,
            prompt_tag_count: count(&conn, "SELECT COUNT(*) FROM prompt_tags")?,
            image_tag_count: count(&conn, "SELECT COUNT(*) FROM image_tags")?,
        })
    })();
    let _ = std::fs::remove_dir_all(&tmp);
    result
}

/// 导出全量备份到指定 ZIP 路径。持有 BkDb 锁（防导出期间数据变动），
/// 库快照用 VACUUM INTO 生成（原子且自动合并 WAL）。
pub fn export<F>(
    app: &tauri::AppHandle,
    bk: &BkDb,
    export_path: &str,
    emit: F,
) -> Result<BackupExportSummary, String>
where
    F: Fn(BackupProgress) + Sync,
{
    emit(BackupProgress {
        stage: "start".into(),
        percent: 0,
        status: "准备导出...".into(),
        detail: None,
    });
    log_info!("备份导出: 开始 to={export_path}");

    let guard = bk.0.lock().map_err(|e| e.to_string())?;
    let images_dir = db::images_dir(app);
    let result = export_core(&guard, &images_dir, Path::new(export_path), &emit);
    match &result {
        Ok(s) => log_info!(
            "备份导出: 完成 prompts={} images={} file={}",
            s.prompts,
            s.images,
            s.file_path
        ),
        Err(e) => log_error!("备份导出: 失败 err={e}"),
    }
    result
}

/// 导出主体（不含 AppHandle，可单测）。
fn export_core<F>(
    conn: &Connection,
    images_dir: &Path,
    export_path: &Path,
    emit: &F,
) -> Result<BackupExportSummary, String>
where
    F: Fn(BackupProgress) + Sync,
{
    let prompts = count(conn, "SELECT COUNT(*) FROM prompts")?;
    let images = count(conn, "SELECT COUNT(*) FROM images")?;

    let tmp = create_temp_dir("paim-backup-export")?;
    let result = (|| -> Result<BackupExportSummary, String> {
        // 1. manifest（5%）
        emit(BackupProgress {
            stage: "manifest".into(),
            percent: 5,
            status: "正在生成备份清单...".into(),
            detail: None,
        });
        let manifest = serde_json::json!({
            "version": "1.0.0",
            "appName": "paim",
            "exportedAt": chrono::Local::now().format("%Y-%m-%dT%H:%M:%S%.3f%:z").to_string(),
            "dataVersion": CURRENT_DATA_VERSION,
            "contents": { "prompts": prompts, "images": images },
        });
        std::fs::write(
            tmp.join(MANIFEST_ENTRY),
            serde_json::to_string_pretty(&manifest).unwrap(),
        )
        .map_err(io_err)?;

        // 2. 库快照 VACUUM INTO（原子、自动合并 WAL；目标文件须不存在）（5% -> 15%）
        emit(BackupProgress {
            stage: "database".into(),
            percent: 10,
            status: "正在导出数据库...".into(),
            detail: None,
        });
        let db_dir = tmp.join("database");
        std::fs::create_dir_all(&db_dir).map_err(io_err)?;
        let snap = db_dir.join("paim.db");
        conn.execute_batch(&format!(
            "VACUUM INTO '{}';",
            snap.to_string_lossy().replace('\'', "''")
        ))
        .map_err(|e| format!("导出数据库快照失败: {e}"))?;

        // 3. 复制图像（15% -> 80%）
        let files_dst = tmp.join("files").join("images");
        copy_dir_with_progress(images_dir, &files_dst, |done, total, name| {
            emit(BackupProgress {
                stage: "images".into(),
                percent: 15 + (done * 65 / total.max(1)) as u32,
                status: format!("正在复制图像文件... ({done}/{total})"),
                detail: Some(name.to_string()),
            });
        })?;

        // 4. 压缩 ZIP（80% -> 99%，逐条目量化进度）
        emit(BackupProgress {
            stage: "compress".into(),
            percent: 80,
            status: "正在压缩备份文件...".into(),
            detail: None,
        });
        write_zip(&tmp, export_path, emit)?;

        emit(BackupProgress {
            stage: "complete".into(),
            percent: 100,
            status: "备份完成！".into(),
            detail: None,
        });
        Ok(BackupExportSummary {
            prompts,
            images,
            file_path: export_path.to_string_lossy().into_owned(),
        })
    })();
    let _ = std::fs::remove_dir_all(&tmp);
    result
}

/// 把暂存目录压缩为 ZIP（条目名用正斜杠相对路径）。
/// 进度逐条目量化（80 -> 99）；图像等已压缩格式用 Stored 直存——deflate 对它们
/// 收益 <1% 却耗掉绝大部分 CPU；manifest/db 维持 deflate（文本与库文件压缩比可观）。
fn write_zip<F>(staging: &Path, out_path: &Path, emit: &F) -> Result<(), String>
where
    F: Fn(BackupProgress) + Sync,
{
    let file = std::fs::File::create(out_path).map_err(|e| format!("创建备份文件失败: {e}"))?;
    let mut zw = zip::ZipWriter::new(file);
    let deflate = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    let store =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);

    let mut files = Vec::new();
    collect_files(staging, staging, &mut files).map_err(|e| format!("扫描暂存目录失败: {e}"))?;
    let total = files.len();
    for (done, (path, rel)) in files.iter().enumerate() {
        let options = if is_precompressed(rel) {
            &store
        } else {
            &deflate
        };
        zw.start_file(rel.as_str(), *options)
            .map_err(|e| format!("写入 ZIP 条目 {rel} 失败: {e}"))?;
        // 流式写入，避免单文件整读进内存
        let src = std::fs::File::open(path).map_err(|e| format!("读取 {rel} 失败: {e}"))?;
        let mut reader = std::io::BufReader::new(src);
        std::io::copy(&mut reader, &mut zw)
            .map_err(|e| format!("写入 ZIP 条目 {rel} 失败: {e}"))?;
        emit(BackupProgress {
            stage: "compress".into(),
            percent: 80 + ((done + 1) * 19 / total.max(1)) as u32,
            status: format!("正在压缩备份文件... ({}/{total})", done + 1),
            detail: Some(rel.clone()),
        });
    }
    zw.finish().map_err(|e| format!("完成 ZIP 写入失败: {e}"))?;
    Ok(())
}

/// 扩展名是否属于已压缩格式（再 deflate 无收益）。
fn is_precompressed(name: &str) -> bool {
    let lower = name.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    matches!(lower.as_str(), "jpg" | "jpeg" | "png" | "webp" | "gif")
}

/// 执行导入：整体替换当前数据（整目录让位 + 换库文件）。与 pm 导入同构。
pub fn import<F>(
    app: &tauri::AppHandle,
    bk: &BkDb,
    zip_path: &str,
    emit: F,
) -> Result<BackupImportSummary, String>
where
    F: Fn(BackupProgress) + Sync,
{
    emit(BackupProgress {
        stage: "start".into(),
        percent: 0,
        status: "准备导入...".into(),
        detail: None,
    });
    log_info!("备份导入: 开始 zip={zip_path}");

    let file = std::fs::File::open(zip_path).map_err(|e| format!("无法打开备份文件: {e}"))?;
    let mut archive = ZipArchive::new(file).map_err(|e| format!("备份文件不是有效的 ZIP: {e}"))?;

    emit(BackupProgress {
        stage: "manifest".into(),
        percent: 3,
        status: "正在解析备份文件...".into(),
        detail: None,
    });
    let root = locate_root(&mut archive)?;
    let manifest = read_manifest(&mut archive, &root)?;
    validate_manifest(&manifest)?;

    let data_dir = db::data_dir(app);
    let db_path = db::user_db_path(app);
    let dir_name = data_dir
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("paim-data")
        .to_string();
    let ts = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let backup_dir = data_dir
        .parent()
        .unwrap_or(&data_dir)
        .join(format!("{dir_name}_{ts}"));
    let had_data_dir = data_dir.exists();

    // 整目录备份（与 pm 一致）：换成内存连接以关闭真连接、释放 paim.db 文件锁。
    // 导入全程持有该锁，占位连接不会被其他命令碰到。
    let mut guard = bk.0.lock().map_err(|e| e.to_string())?;
    if had_data_dir {
        *guard = Connection::open_in_memory().map_err(|e| format!("切换临时连接失败: {e}"))?;
        log_info!("备份导入: 已切换内存连接，开始让位改名 from={data_dir:?} to={backup_dir:?}");
        if let Err(e) = std::fs::rename(&data_dir, &backup_dir) {
            log_error!("备份导入: 数据目录让位改名失败 err={e}");
            *guard = open_app_db(&db_path)?;
            return Err(format!(
                "备份原数据目录失败，请关闭可能占用数据目录的程序（如资源管理器窗口）后重试: {e}"
            ));
        }
        log_info!("备份导入: 让位改名完成");
    }

    let mut run = || -> Result<BackupImportSummary, String> {
        std::fs::create_dir_all(&data_dir).map_err(|e| format!("创建数据目录失败: {e}"))?;
        let images_dir = db::images_dir(app);
        std::fs::create_dir_all(&images_dir).map_err(|e| format!("创建图像目录失败: {e}"))?;
        let tmp = create_temp_dir("paim-backup-import")?;
        let result = import_inner(
            &mut guard,
            &mut archive,
            &root,
            &data_dir,
            &db_path,
            &images_dir,
            &tmp,
            &emit,
        );
        let _ = std::fs::remove_dir_all(&tmp);
        let (prompts, images, thumbnail_failures) = result?;
        log_info!(
            "备份导入: 完成 prompts={prompts} images={images} 缩略图失败={thumbnail_failures}"
        );
        emit(BackupProgress {
            stage: "complete".into(),
            percent: 100,
            status: "导入完成！".into(),
            detail: None,
        });
        Ok(BackupImportSummary {
            prompts,
            images,
            thumbnail_failures,
            backup_dir: if had_data_dir {
                backup_dir.to_string_lossy().into_owned()
            } else {
                String::new()
            },
        })
    };

    let outcome = run();
    match outcome {
        Ok(summary) => Ok(summary),
        Err(e) => {
            // 回滚：关新连接（换占位）→ 删半成品目录 → 备份目录归位 → 重开原库
            let placeholder = Connection::open_in_memory()
                .map_err(|pe| format!("{e}；回滚时切换临时连接也失败: {pe}"))?;
            *guard = placeholder;
            let _ = std::fs::remove_dir_all(&data_dir);
            let rename_back_ok = if had_data_dir {
                std::fs::rename(&backup_dir, &data_dir).is_ok()
            } else {
                true
            };
            match open_app_db(&db_path) {
                Ok(conn) => {
                    *guard = conn;
                    if rename_back_ok {
                        Err(format!("{e}（已回滚到原数据）"))
                    } else {
                        Err(format!(
                            "{e}；自动归位失败，原数据完整保留在 {}，请手动改名为「{}」后重启应用",
                            backup_dir.display(),
                            dir_name
                        ))
                    }
                }
                Err(re_err) => Err(format!("{e}；回滚后重开数据库失败: {re_err}，请重启应用")),
            }
        }
    }
}

/// 导入主体：解包落文件 → 换库文件重开 → 重建缩略图，返回统计。
#[allow(clippy::too_many_arguments)]
fn import_inner<F>(
    guard: &mut Connection,
    archive: &mut ZipArchive<std::fs::File>,
    root: &str,
    data_dir: &Path,
    db_path: &Path,
    images_dir: &Path,
    tmp: &Path,
    emit: &F,
) -> Result<(i64, i64, usize), String>
where
    F: Fn(BackupProgress) + Sync,
{
    let image_prefix = format!("{root}{IMAGES_ENTRY_PREFIX}");
    let total_images = count_image_entries(archive, &image_prefix);

    // 解包：数据库进临时目录，图像流式写入 images/（8% -> 55%）
    let snap_db = tmp.join("paim.db");
    let mut copied = 0usize;
    let mut found_db = false;
    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| format!("读取 ZIP 条目失败: {e}"))?;
        if entry.is_dir() {
            continue;
        }
        let norm = entry.name().replace('\\', "/");
        if norm == format!("{root}{DB_ENTRY}") {
            let mut out = std::fs::File::create(&snap_db).map_err(io_err)?;
            std::io::copy(&mut entry, &mut out).map_err(io_err)?;
            found_db = true;
            continue;
        }
        let Some(img_rel) = norm.strip_prefix(&image_prefix) else {
            continue;
        };
        if img_rel.is_empty() || !is_safe_rel_path(img_rel) {
            continue;
        }
        let dest = images_dir.join(img_rel);
        let parent = dest
            .parent()
            .ok_or_else(|| format!("无效的图像路径: {img_rel}"))?;
        std::fs::create_dir_all(parent).map_err(io_err)?;
        let mut out = std::fs::File::create(&dest).map_err(io_err)?;
        std::io::copy(&mut entry, &mut out).map_err(io_err)?;
        copied += 1;
        emit(BackupProgress {
            stage: "images".into(),
            percent: 8 + (copied * 47 / total_images.max(1)) as u32,
            status: format!("正在恢复图像文件... ({copied}/{total_images})"),
            detail: Some(img_rel.to_string()),
        });
    }
    if !found_db {
        return Err("备份缺少数据库文件 database/paim.db".into());
    }

    // 换库文件并重开（55% -> 65%）：VACUUM INTO 产物是独立主库文件（无 WAL 伴生），
    // 直接覆盖 db_path；db::init 打开时自动跑迁移升级旧备份。
    emit(BackupProgress {
        stage: "database".into(),
        percent: 60,
        status: "正在写入数据...".into(),
        detail: None,
    });
    std::fs::copy(&snap_db, db_path).map_err(|e| format!("写入数据库文件失败: {e}"))?;
    *guard = open_app_db(db_path)?;

    let prompts = count(guard, "SELECT COUNT(*) FROM prompts")?;
    let images = count(guard, "SELECT COUNT(*) FROM images")?;

    // 缩略图全量重建（65% -> 98%）；thumbnails 目录约定为数据目录下的 thumbnails/
    emit(BackupProgress {
        stage: "thumbnails".into(),
        percent: 65,
        status: "正在重建缩略图...".into(),
        detail: None,
    });
    let thumbs_root = data_dir.join("thumbnails");
    let summary = crate::domain::thumbnail_service::rebuild_all(
        data_dir,
        &thumbs_root,
        guard,
        |done, total, file_name| {
            emit(BackupProgress {
                stage: "thumbnails".into(),
                percent: 65 + (done * 33 / total.max(1)) as u32,
                status: format!("正在重建缩略图... ({done}/{total})"),
                detail: Some(file_name.to_string()),
            });
        },
    )?;
    Ok((prompts, images, summary.failed))
}

/// 读取 manifest.json 并解析。
fn read_manifest(
    archive: &mut ZipArchive<std::fs::File>,
    root: &str,
) -> Result<BackupManifest, String> {
    let content = read_entry_to_string(archive, &format!("{root}{MANIFEST_ENTRY}"))?;
    serde_json::from_str(&content).map_err(|_| "manifest.json 格式无效".to_string())
}

fn validate_manifest(m: &BackupManifest) -> Result<(), String> {
    if m.app_name != "paim" {
        return Err("不是 paim 导出的备份文件".into());
    }
    let v = m.data_version.unwrap_or(1);
    if v > CURRENT_DATA_VERSION {
        return Err(format!(
            "备份数据格式版本过新：{v}（当前支持 ≤{CURRENT_DATA_VERSION}），请升级应用后再导入"
        ));
    }
    Ok(())
}

#[cfg(test)]
#[path = "paim_backup_service.test.rs"]
mod tests;
