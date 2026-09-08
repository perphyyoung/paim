//! pm(prompt-manager)全量备份导入服务。
//! 备份包结构（prompt-manager 的 ExportFullBackupService 导出）：
//!   manifest.json + database/prompt-manager.db + files/images/{YYYYMM}/。
//! 语义与 pm 的导入一致：整体替换当前数据。导入前整个数据目录改名让位
//! （同级 `paim-data_{时间戳}`，含图像与缩略图，改回原名即可直接使用），
//! 原路径重建后灌入备份数据；数据库写入为 ATTACH + 单事务，失败自动回滚
//! 目录与数据。应用数据库连接经内存占位连接换绑（见 import 注释）。

use rusqlite::Connection;
use serde::Deserialize;
use std::path::Path;
use zip::ZipArchive;

use crate::domain::backup_common::{
    count_image_entries, create_temp_dir, extract_entry, io_err, is_safe_rel_path, locate_root,
    open_app_db, read_entry_to_string, BackupImportSummary, BackupInfo, BackupProgress,
    MANIFEST_ENTRY,
};
use crate::infra::db::{self, BkDb};
use crate::infra::time::normalize_ts;
use crate::{log_error, log_info};

/// 当前支持的数据格式版本（与 pm 的 CURRENT_DATA_VERSION 一致）。
const SUPPORTED_DATA_VERSION: i64 = 1;

/// pm 备份包内的固定布局（manifest 条目名复用 backup_common 的 MANIFEST_ENTRY）。
const DB_ENTRY: &str = "database/prompt-manager.db";
const IMAGES_ENTRY_PREFIX: &str = "files/images/";

/// manifest.json 中导入所需字段（字段名与 pm 的 IBackupManifest 一致，其余字段忽略）。
#[derive(Debug, Deserialize)]
struct BackupManifest {
    #[serde(rename = "appName")]
    app_name: String,
    #[serde(rename = "exportedAt", default)]
    exported_at: String,
    /// pm 导出恒为 1；缺失按 1 处理（与 pm 导入行为一致）。
    #[serde(rename = "dataVersion", default)]
    data_version: Option<i64>,
}

/// 备份内容概览、导入摘要与进度事件复用 backup_common 的共享类型（pm/paim 同构）。

/// 解析备份包，返回内容概览（不改动任何本地数据）。
pub fn inspect(zip_path: &str) -> Result<BackupInfo, String> {
    let file = std::fs::File::open(zip_path).map_err(|e| format!("无法打开备份文件: {e}"))?;
    let mut archive = ZipArchive::new(file).map_err(|e| format!("备份文件不是有效的 ZIP: {e}"))?;

    let root = locate_root(&mut archive)?;
    let manifest = read_manifest(&mut archive, &root)?;
    validate_manifest(&manifest)?;

    let tmp = create_temp_dir("paim-pm-import")?;
    let result = (|| {
        let pm_db = tmp.join("prompt-manager.db");
        extract_entry(&mut archive, &format!("{root}{DB_ENTRY}"), &pm_db)?;
        let conn = Connection::open_with_flags(&pm_db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(|e| format!("打开备份数据库失败: {e}"))?;
        let count = |sql: &str| -> Result<i64, String> {
            conn.query_row(sql, [], |r| r.get(0))
                .map_err(|e| format!("统计备份数据失败: {e}"))
        };
        Ok(BackupInfo {
            app: "pm".into(),
            exported_at: manifest.exported_at.clone(),
            prompt_count: count("SELECT COUNT(*) FROM prompts")?,
            image_count: count("SELECT COUNT(*) FROM images")?,
            trashed_prompt_count: count("SELECT COUNT(*) FROM prompts WHERE is_deleted = 1")?,
            trashed_image_count: count("SELECT COUNT(*) FROM images WHERE is_deleted = 1")?,
            prompt_tag_count: count("SELECT COUNT(*) FROM prompt_tags")?,
            image_tag_count: count("SELECT COUNT(*) FROM image_tags")?,
        })
    })();
    let _ = std::fs::remove_dir_all(&tmp);
    result
}

/// 执行导入：整体替换当前数据。与 pm 一致——导入前整个数据目录改名让位
/// （同级 `paim-data_{时间戳}`，含图像与缩略图，改回原名即可直接使用），
/// 原路径重建后灌入备份数据；失败自动回滚（删半成品、备份目录归位、重开原库）。
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
    log_info!("导入: 开始 zip={zip_path}");

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

    // 整目录备份（与 pm 一致）：换成内存连接以关闭真连接、释放 paim.db 文件锁
    //（关闭时 WAL 自动合并，备份目录里的库文件即完整）。导入全程持有该锁，
    // 占位连接不会被其他命令碰到。
    let mut guard = bk.0.lock().map_err(|e| e.to_string())?;
    if had_data_dir {
        *guard = Connection::open_in_memory().map_err(|e| format!("切换临时连接失败: {e}"))?;
        log_info!("导入: 已切换内存连接，开始让位改名 from={data_dir:?} to={backup_dir:?}");
        if let Err(e) = std::fs::rename(&data_dir, &backup_dir) {
            log_error!("导入: 数据目录让位改名失败 err={e}");
            *guard = open_app_db(&db_path)?;
            return Err(format!(
                "备份原数据目录失败，请关闭可能占用数据目录的程序（如资源管理器窗口）后重试: {e}"
            ));
        }
        log_info!("导入: 让位改名完成");
    }

    let mut run = || -> Result<BackupImportSummary, String> {
        std::fs::create_dir_all(&data_dir).map_err(|e| format!("创建数据目录失败: {e}"))?;
        *guard = open_app_db(&db_path)?;
        let images_dir = db::images_dir(app);
        std::fs::create_dir_all(&images_dir).map_err(|e| format!("创建图像目录失败: {e}"))?;
        let tmp = create_temp_dir("paim-pm-import")?;
        let result = import_inner(app, &guard, &mut archive, &root, &images_dir, &tmp, &emit);
        let _ = std::fs::remove_dir_all(&tmp);
        let (prompts, images, thumbnail_failures) = result?;
        log_info!("导入: 完成 prompts={prompts} images={images} 缩略图失败={thumbnail_failures}");
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

/// 导入主体：解包落文件 → 事务替换数据 → 重建缩略图，返回统计。
fn import_inner<F>(
    app: &tauri::AppHandle,
    conn: &Connection,
    archive: &mut ZipArchive<std::fs::File>,
    root: &str,
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
    let pm_db = tmp.join("prompt-manager.db");
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
            let mut out = std::fs::File::create(&pm_db).map_err(io_err)?;
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
        return Err("备份缺少数据库文件 prompt-manager.db".into());
    }

    // 数据整体替换（55% -> 65%）
    emit(BackupProgress {
        stage: "database".into(),
        percent: 60,
        status: "正在写入数据...".into(),
        detail: None,
    });
    let (prompts, images) = replace_tables(conn, &pm_db)?;

    // 缩略图全量重建（65% -> 98%）
    emit(BackupProgress {
        stage: "thumbnails".into(),
        percent: 65,
        status: "正在重建缩略图...".into(),
        detail: None,
    });
    let thumbnail_failures = regenerate_thumbnails(app, conn, emit)?;
    Ok((prompts, images, thumbnail_failures))
}

/// 单事务整体替换：清空当前业务表，从 ATTACH 的 pm 库原样灌入（保留 id 与回收站数据）。
fn replace_tables(conn: &Connection, pm_db: &Path) -> Result<(i64, i64), String> {
    conn.execute_batch(&format!(
        "ATTACH DATABASE '{}' AS pm_import;",
        pm_db.to_string_lossy().replace('\'', "''")
    ))
    .map_err(|e| format!("挂载备份数据库失败: {e}"))?;

    let run = || -> rusqlite::Result<()> {
        conn.execute_batch("BEGIN IMMEDIATE;")?;
        // 子表在前，foreign_keys=ON 下删除顺序安全
        conn.execute_batch(
            "DELETE FROM prompt_tag_relations;
             DELETE FROM image_tag_relations;
             DELETE FROM prompt_image_relations;
             DELETE FROM prompt_tags;
             DELETE FROM image_tags;
             DELETE FROM prompt_tag_groups;
             DELETE FROM image_tag_groups;
             DELETE FROM prompts;
             DELETE FROM images;
             DELETE FROM db_version;",
        )?;
        // prompts / images 是 relations 的父表；foreign_keys=ON 下必须先插入父表，
        // 故在 REPLACE_SQL 之前写入。其 created_at/updated_at/deleted_at 经 normalize_ts 规整为 ISO 8601 UTC，
        // 故不走整表拷贝，改为 Rust 侧逐行读取转换后写入（pm_import 此时仍挂载）
        import_prompts(conn)?;
        import_images(conn)?;
        conn.execute_batch(REPLACE_SQL)?;
        // 必须显式提交：否则数据停留在未提交事务里，仅本连接可见，断开即回滚
        conn.execute_batch("COMMIT;")
    };
    let outcome = run();
    if outcome.is_err() {
        let _ = conn.execute_batch("ROLLBACK;");
    }
    let _ = conn.execute_batch("DETACH DATABASE pm_import;");
    outcome.map_err(|e| format!("写入 pm 数据失败: {e}"))?;

    let prompts: i64 = conn
        .query_row("SELECT COUNT(*) FROM prompts", [], |r| r.get(0))
        .map_err(|e| format!("统计导入结果失败: {e}"))?;
    let images: i64 = conn
        .query_row("SELECT COUNT(*) FROM images", [], |r| r.get(0))
        .map_err(|e| format!("统计导入结果失败: {e}"))?;
    Ok((prompts, images))
}

/// 从 pm_import 灌入全部业务表（显式列名；两边列结构一致）。
/// 注意：prompts / images 的时间字段在此不走整表拷贝——它们的 created_at/updated_at/deleted_at 可能为非规范的
/// 斜杠本地格式（pm 早期备份），需经 `normalize_ts` 规整为 ISO 8601 UTC 后再写入，故由
/// `import_prompts` / `import_images` 在 Rust 侧逐行转换，不在此批量 INSERT...SELECT 中处理。
const REPLACE_SQL: &str = "
INSERT INTO prompt_tag_groups (id, name, sort_order, created_at, updated_at)
  SELECT id, name, sort_order, created_at, updated_at FROM pm_import.prompt_tag_groups;
INSERT INTO prompt_tags (id, name, group_id, created_at, updated_at)
  SELECT id, name, group_id, created_at, updated_at FROM pm_import.prompt_tags;
INSERT INTO image_tag_groups (id, name, sort_order, created_at, updated_at)
  SELECT id, name, sort_order, created_at, updated_at FROM pm_import.image_tag_groups;
INSERT INTO image_tags (id, name, group_id, created_at, updated_at)
  SELECT id, name, group_id, created_at, updated_at FROM pm_import.image_tags;
INSERT INTO prompt_tag_relations (prompt_id, tag_id)
  SELECT prompt_id, tag_id FROM pm_import.prompt_tag_relations;
INSERT INTO image_tag_relations (image_id, tag_id)
  SELECT image_id, tag_id FROM pm_import.image_tag_relations;
INSERT INTO prompt_image_relations (prompt_id, image_id, sort_order)
  SELECT prompt_id, image_id, sort_order FROM pm_import.prompt_image_relations;
INSERT INTO db_version (version, applied_at)
  SELECT version, applied_at FROM pm_import.db_version;
";

/// 从 pm_import 灌入 prompts，并把 created_at/updated_at/deleted_at 规整为 ISO 8601 UTC。
fn import_prompts(conn: &Connection) -> rusqlite::Result<()> {
    let mut stmt = conn.prepare(
        "SELECT id, title, content, content_translate, created_at, updated_at, is_deleted, deleted_at, is_favorite, is_safe, note FROM pm_import.prompts",
    )?;
    let rows: Vec<(
        String,
        String,
        String,
        String,
        String,
        String,
        i64,
        Option<String>,
        i64,
        i64,
        String,
    )> = stmt
        .query_map([], |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
                r.get(6)?,
                r.get(7)?,
                r.get(8)?,
                r.get(9)?,
                r.get(10)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut insert = conn.prepare(
        "INSERT INTO prompts (id, title, content, content_translate, created_at, updated_at, is_deleted, deleted_at, is_favorite, is_safe, note) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
    )?;
    for (id, title, content, ct, ca, ua, del, da, fav, safe, note) in rows {
        insert.execute(rusqlite::params![
            id,
            title,
            content,
            ct,
            normalize_ts(&ca),
            normalize_ts(&ua),
            del,
            da.as_deref().map(normalize_ts),
            fav,
            safe,
            note
        ])?;
    }
    Ok(())
}

/// 从 pm_import 灌入 images，并把 created_at/updated_at/deleted_at 规整为 ISO 8601 UTC。
fn import_images(conn: &Connection) -> rusqlite::Result<()> {
    let mut stmt = conn.prepare(
        "SELECT id, file_name, stored_name, relative_path, thumbnail_path, md5, width, height, file_size, gen_params, is_deleted, deleted_at, is_favorite, is_safe, created_at, updated_at, note FROM pm_import.images",
    )?;
    let rows: Vec<(
        String,
        String,
        String,
        String,
        Option<String>,
        Option<String>,
        Option<i64>,
        Option<i64>,
        Option<i64>,
        String,
        i64,
        Option<String>,
        i64,
        i64,
        String,
        String,
        String,
    )> = stmt
        .query_map([], |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
                r.get(6)?,
                r.get(7)?,
                r.get(8)?,
                r.get(9)?,
                r.get(10)?,
                r.get(11)?,
                r.get(12)?,
                r.get(13)?,
                r.get(14)?,
                r.get(15)?,
                r.get(16)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut insert = conn.prepare(
        "INSERT INTO images (id, file_name, stored_name, relative_path, thumbnail_path, md5, width, height, file_size, gen_params, is_deleted, deleted_at, is_favorite, is_safe, created_at, updated_at, note) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17)",
    )?;
    for (
        id,
        file_name,
        stored_name,
        relative_path,
        thumb,
        md5,
        width,
        height,
        file_size,
        gen,
        del,
        da,
        fav,
        safe,
        ca,
        ua,
        note,
    ) in rows
    {
        insert.execute(rusqlite::params![
            id,
            file_name,
            stored_name,
            relative_path,
            thumb,
            md5,
            width.unwrap_or(0),
            height.unwrap_or(0),
            file_size.unwrap_or(0),
            gen,
            del,
            da.as_deref().map(normalize_ts),
            fav,
            safe,
            normalize_ts(&ca),
            normalize_ts(&ua),
            note
        ])?;
    }
    Ok(())
}

/// 全量重建缩略图：转调 thumbnail_service（补缺失语义与 pm 一致），
/// 进度映射到导入进度事件的 thumbnails 阶段，返回失败数。
fn regenerate_thumbnails<F>(
    app: &tauri::AppHandle,
    conn: &Connection,
    emit: &F,
) -> Result<usize, String>
where
    F: Fn(BackupProgress) + Sync,
{
    let data_dir = db::data_dir(app);
    let thumbs_root = db::thumbnails_dir(app);
    let summary = crate::domain::thumbnail_service::rebuild_all(
        &data_dir,
        &thumbs_root,
        conn,
        |done, total, file_name| {
            emit(BackupProgress {
                stage: "thumbnails".into(),
                percent: 65 + (done * 33 / total.max(1)) as u32,
                status: format!("正在重建缩略图... ({done}/{total})"),
                detail: Some(file_name.to_string()),
            });
        },
    )?;
    Ok(summary.failed)
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
    if m.app_name != "prompt-manager" {
        return Err("不是 prompt-manager 导出的备份文件".into());
    }
    if let Some(v) = m.data_version {
        if v != SUPPORTED_DATA_VERSION {
            return Err(format!(
                "备份数据格式版本不兼容：{v}（当前支持 {SUPPORTED_DATA_VERSION}）"
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "pm_backup_service.test.rs"]
mod tests;
