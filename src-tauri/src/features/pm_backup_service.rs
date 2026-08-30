//! pm(prompt-manager)全量备份导入服务。
//! 备份包结构（prompt-manager 的 ExportFullBackupService 导出）：
//!   manifest.json + database/prompt-manager.db + files/images/{YYYYMM}/。
//! 语义与 pm 的导入一致：整体替换当前数据。导入前整个数据目录改名让位
//! （同级 `paim-data_{时间戳}`，含图像与缩略图，改回原名即可直接使用），
//! 原路径重建后灌入备份数据；数据库写入为 ATTACH + 单事务，失败自动回滚
//! 目录与数据。应用数据库连接经内存占位连接换绑（见 import 注释）。

use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use zip::ZipArchive;

use crate::db::{self, BkDb};

/// 进度推送事件名（前端 listen 用）。
pub const PROGRESS_EVENT: &str = "pm-import-progress";

/// 当前支持的数据格式版本（与 pm 的 CURRENT_DATA_VERSION 一致）。
const SUPPORTED_DATA_VERSION: i64 = 1;

/// pm 备份包内的固定布局。
const MANIFEST_ENTRY: &str = "manifest.json";
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

/// 备份内容概览（供确认弹窗展示）。
#[derive(Debug, Serialize)]
pub struct PmBackupInfo {
    pub exported_at: String,
    pub prompt_count: i64,
    pub image_count: i64,
    pub trashed_image_count: i64,
    pub prompt_tag_count: i64,
    pub image_tag_count: i64,
}

/// 导入结果摘要。
#[derive(Debug, Serialize)]
pub struct PmImportSummary {
    pub prompts: i64,
    pub images: i64,
    pub thumbnail_failures: usize,
    /// 原数据目录的备份位置（整体改名让位）；无原数据时为空串。
    pub backup_dir: String,
}

/// 导入进度推送载荷。
#[derive(Debug, Serialize, Clone)]
pub struct ImportProgress {
    pub stage: String,
    pub percent: u32,
    pub status: String,
    pub detail: Option<String>,
}

/// 解析备份包，返回内容概览（不改动任何本地数据）。
pub fn inspect(zip_path: &str) -> Result<PmBackupInfo, String> {
    let file = std::fs::File::open(zip_path).map_err(|e| format!("无法打开备份文件: {e}"))?;
    let mut archive = ZipArchive::new(file).map_err(|e| format!("备份文件不是有效的 ZIP: {e}"))?;

    let root = locate_root(&mut archive)?;
    let manifest = read_manifest(&mut archive, &root)?;
    validate_manifest(&manifest)?;

    let tmp = create_temp_dir()?;
    let result = (|| {
        let pm_db = tmp.join("prompt-manager.db");
        extract_entry(&mut archive, &format!("{root}{DB_ENTRY}"), &pm_db)?;
        let conn = Connection::open_with_flags(
            &pm_db,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .map_err(|e| format!("打开备份数据库失败: {e}"))?;
        let count = |sql: &str| -> Result<i64, String> {
            conn.query_row(sql, [], |r| r.get(0))
                .map_err(|e| format!("统计备份数据失败: {e}"))
        };
        Ok(PmBackupInfo {
            exported_at: manifest.exported_at.clone(),
            prompt_count: count("SELECT COUNT(*) FROM prompts")?,
            image_count: count("SELECT COUNT(*) FROM images")?,
            trashed_image_count: count("SELECT COUNT(*) FROM images WHERE is_deleted = 1")?,
            prompt_tag_count: count("SELECT COUNT(*) FROM prompt_tags")?,
            image_tag_count: count("SELECT COUNT(*) FROM image_tags")?,
        })
    })();
    let _ = std::fs::remove_dir_all(&tmp);
    result
}

/// 在指定路径打开应用数据库（含建表），返回裸连接供装入 BkDb。
fn open_app_db(path: &Path) -> Result<Connection, String> {
    db::init(path.to_path_buf())
        .map_err(|e| format!("初始化数据库失败: {e}"))?
        .0
        .into_inner()
        .map_err(|_| "数据库句柄已损坏".to_string())
}

/// 执行导入：整体替换当前数据。与 pm 一致——导入前整个数据目录改名让位
/// （同级 `paim-data_{时间戳}`，含图像与缩略图，改回原名即可直接使用），
/// 原路径重建后灌入备份数据；失败自动回滚（删半成品、备份目录归位、重开原库）。
pub fn import<F>(app: &tauri::AppHandle, bk: &BkDb, zip_path: &str, emit: F) -> Result<PmImportSummary, String>
where
    F: Fn(ImportProgress) + Sync,
{
    emit(ImportProgress {
        stage: "start".into(),
        percent: 0,
        status: "准备导入...".into(),
        detail: None,
    });

    let file = std::fs::File::open(zip_path).map_err(|e| format!("无法打开备份文件: {e}"))?;
    let mut archive = ZipArchive::new(file).map_err(|e| format!("备份文件不是有效的 ZIP: {e}"))?;

    emit(ImportProgress {
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
        if let Err(e) = std::fs::rename(&data_dir, &backup_dir) {
            *guard = open_app_db(&db_path)?;
            return Err(format!(
                "备份原数据目录失败，请关闭可能占用数据目录的程序（如资源管理器窗口）后重试: {e}"
            ));
        }
    }

    let mut run = || -> Result<PmImportSummary, String> {
        std::fs::create_dir_all(&data_dir).map_err(|e| format!("创建数据目录失败: {e}"))?;
        *guard = open_app_db(&db_path)?;
        let images_dir = db::images_dir(app);
        std::fs::create_dir_all(&images_dir).map_err(|e| format!("创建图像目录失败: {e}"))?;
        let tmp = create_temp_dir()?;
        let result = import_inner(app, &guard, &mut archive, &root, &images_dir, &tmp, &emit);
        let _ = std::fs::remove_dir_all(&tmp);
        let (prompts, images, thumbnail_failures) = result?;
        emit(ImportProgress {
            stage: "complete".into(),
            percent: 100,
            status: "导入完成！".into(),
            detail: None,
        });
        Ok(PmImportSummary {
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
    F: Fn(ImportProgress) + Sync,
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
        emit(ImportProgress {
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
    emit(ImportProgress {
        stage: "database".into(),
        percent: 60,
        status: "正在写入数据...".into(),
        detail: None,
    });
    let (prompts, images) = replace_tables(conn, &pm_db)?;

    // 缩略图全量重建（65% -> 98%）
    emit(ImportProgress {
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
const REPLACE_SQL: &str = "
INSERT INTO prompts (id, title, content, content_translate, created_at, updated_at, is_deleted, deleted_at, is_favorite, is_safe, note)
  SELECT id, title, content, content_translate, created_at, updated_at, is_deleted, deleted_at, is_favorite, is_safe, note FROM pm_import.prompts;
INSERT INTO images (id, file_name, stored_name, relative_path, thumbnail_path, md5, width, height, file_size, gen_params, is_deleted, deleted_at, is_favorite, is_safe, created_at, updated_at, note)
  SELECT id, file_name, stored_name, relative_path, thumbnail_path, md5, width, height, file_size, gen_params, is_deleted, deleted_at, is_favorite, is_safe, created_at, updated_at, note FROM pm_import.images;
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

/// 缩略图生成工作线程数上限：解码是 CPU 密集的独立任务，并行近似线性加速
/// （参照 pm 的并发模型）；设上限控制峰值内存（每线程持有一张解码位图）。
const THUMB_WORKERS_MAX: usize = 8;

/// 单图缩略图生成：读原图 → 解码 → 200×200 居中裁剪 → 存
/// thumbnails/{YYYYMM}/thumb_{stored_name 词干}.jpg，
/// 返回要写回 images.thumbnail_path 的相对路径；失败原因以 Err 返回。
fn build_thumbnail(data_dir: &Path, thumbs_root: &Path, rel: &str) -> Result<String, String> {
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

    let img = image::open(data_dir.join(rel)).map_err(|e| format!("读取图像失败: {e}"))?;
    let thumb = crate::features::image_service::make_center_thumb(&img)
        .map_err(|e| format!("生成缩略图失败: {e}"))?;
    std::fs::create_dir_all(&thumb_dir).map_err(io_err)?;
    // relative_path 以 stored_name 结尾，取词干即 pm 的缩略图命名
    let stem = rel_path
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| format!("无效的图像路径: {rel}"))?;
    let name = format!("thumb_{stem}.jpg");
    thumb
        .save(thumb_dir.join(&name))
        .map_err(|e| format!("保存缩略图失败: {e}"))?;
    Ok(format!("{thumb_rel_prefix}/{name}"))
}

/// 全量重建缩略图（paim 布局：thumbnails/{YYYYMM}/thumb_{stored_name 词干}.jpg），
/// 并重写 thumbnail_path；解码失败的记录置空并计数。返回失败数。
/// 参照 pm 的并发模型：多工作线程并发生成，结束后单事务批量回写。
fn regenerate_thumbnails<F>(
    app: &tauri::AppHandle,
    conn: &Connection,
    emit: &F,
) -> Result<usize, String>
where
    F: Fn(ImportProgress) + Sync,
{
    let data_dir = db::data_dir(app);
    let thumbs_root = db::thumbnails_dir(app);
    std::fs::create_dir_all(&thumbs_root).map_err(io_err)?;

    // 先收集再处理，避免边查询边更新同一张表
    let rows: Vec<(String, String)> = {
        let mut stmt = conn
            .prepare("SELECT id, relative_path FROM images")
            .map_err(|e| format!("读取图像记录失败: {e}"))?;
        let mapped = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .map_err(|e| format!("读取图像记录失败: {e}"))?;
        mapped
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("读取图像记录失败: {e}"))?
    };
    let total = rows.len();
    if total == 0 {
        return Ok(0);
    }

    // 并发生成：游标领任务，结果集中收集；进度计数共享，由完成线程直接推送
    let next = AtomicUsize::new(0);
    let completed = AtomicUsize::new(0);
    let results: Mutex<Vec<(String, Result<String, String>)>> = Mutex::new(Vec::with_capacity(total));
    let workers = total
        .min(
            std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(4),
        )
        .min(THUMB_WORKERS_MAX);
    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| loop {
                let idx = next.fetch_add(1, Ordering::Relaxed);
                if idx >= rows.len() {
                    break;
                }
                let (id, rel) = &rows[idx];
                let outcome = build_thumbnail(&data_dir, &thumbs_root, rel);
                results.lock().unwrap().push((id.clone(), outcome));
                let done = completed.fetch_add(1, Ordering::Relaxed) + 1;
                emit(ImportProgress {
                    stage: "thumbnails".into(),
                    percent: 65 + (done * 33 / total) as u32,
                    status: format!("正在重建缩略图... ({done}/{total})"),
                    detail: None,
                });
            });
        }
    });

    // 单事务批量回写 thumbnail_path（失败置 NULL 并计数）
    let outcomes = results
        .into_inner()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    conn.execute_batch("BEGIN IMMEDIATE;")
        .map_err(|e| format!("开启缩略图回写事务失败: {e}"))?;
    let mut failures = 0usize;
    for (id, outcome) in outcomes {
        let thumb_rel = match outcome {
            Ok(rel) => Some(rel),
            Err(msg) => {
                failures += 1;
                log::warn!("缩略图重建失败 id={id}: {msg}");
                None
            }
        };
        if let Err(e) = conn.execute(
            "UPDATE images SET thumbnail_path = ?1 WHERE id = ?2",
            rusqlite::params![thumb_rel, id],
        ) {
            let _ = conn.execute_batch("ROLLBACK;");
            return Err(format!("更新缩略图路径失败: {e}"));
        }
    }
    conn.execute_batch("COMMIT;")
        .map_err(|e| format!("提交缩略图回写事务失败: {e}"))?;
    Ok(failures)
}

/// 在包内定位 manifest.json，返回其目录前缀（"" 或 "dir/"）。
/// 兼容 Unix `zip -r` 打包时多出的一层临时目录包裹（pm 自身导入不支持该格式）。
fn locate_root(archive: &mut ZipArchive<std::fs::File>) -> Result<String, String> {
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

/// 读取 manifest.json 并解析。
fn read_manifest(archive: &mut ZipArchive<std::fs::File>, root: &str) -> Result<BackupManifest, String> {
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

/// 按归一化路径读取一个条目为字符串。
fn read_entry_to_string(
    archive: &mut ZipArchive<std::fs::File>,
    normalized: &str,
) -> Result<String, String> {
    let raw = raw_name_of(archive, normalized)
        .ok_or_else(|| format!("备份缺少 {normalized}"))?;
    let mut entry = archive
        .by_name(&raw)
        .map_err(|e| format!("读取 {normalized} 失败: {e}"))?;
    let mut buf = String::new();
    entry
        .read_to_string(&mut buf)
        .map_err(|e| format!("读取 {normalized} 失败: {e}"))?;
    Ok(buf)
}

/// 把包内指定条目解压为单个文件（用于 pm 数据库）。
fn extract_entry(
    archive: &mut ZipArchive<std::fs::File>,
    normalized: &str,
    dest: &Path,
) -> Result<(), String> {
    let raw = raw_name_of(archive, normalized)
        .ok_or_else(|| format!("备份缺少 {normalized}"))?;
    let mut entry = archive
        .by_name(&raw)
        .map_err(|e| format!("读取 {normalized} 失败: {e}"))?;
    let mut out = std::fs::File::create(dest).map_err(|e| format!("创建临时文件失败: {e}"))?;
    std::io::copy(&mut entry, &mut out)
        .map_err(|e| format!("解压 {normalized} 失败: {e}"))?;
    Ok(())
}

/// 按归一化路径查找条目的原始名（Windows Compress-Archive 可能用反斜杠存储条目名）。
fn raw_name_of(archive: &mut ZipArchive<std::fs::File>, normalized: &str) -> Option<String> {
    for i in 0..archive.len() {
        if let Ok(f) = archive.by_index(i) {
            if f.name().replace('\\', "/") == normalized {
                return Some(f.name().to_string());
            }
        }
    }
    None
}

/// 统计图像目录下的文件条目数（用于进度）。
fn count_image_entries(archive: &mut ZipArchive<std::fs::File>, image_prefix: &str) -> usize {
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

/// 条目相对路径是否安全（拒绝空段/./.. /盘符），防 zip-slip。
fn is_safe_rel_path(rel: &str) -> bool {
    !rel.split('/').any(|seg| seg.is_empty() || seg == "." || seg == ".." || seg.ends_with(':'))
}

fn create_temp_dir() -> Result<PathBuf, String> {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = std::env::temp_dir().join(format!("paim-pm-import-{nanos}"));
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建临时目录失败: {e}"))?;
    Ok(dir)
}

fn io_err(e: std::io::Error) -> String {
    format!("写入文件失败: {e}")
}


#[cfg(test)]
mod tests;
