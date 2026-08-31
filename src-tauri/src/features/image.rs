//! 图像命令层：薄适配，从 managed state 取连接，转调领域服务。
//! 路径 `features::image::`，与提示词侧 `features::prompt::` 对称。

use crate::db::BkDb;
use crate::error::AppError;
use crate::features::image_service::{
    self, Image, ImageTag, ImportBatchResult, ImportResult, LinkedPrompt,
};
use crate::features::prompt_service;
use crate::features::thumbnail_service::{self, EnsureResult, RebuildProgress, RebuildSummary};

use rusqlite::OptionalExtension;
use tauri::{Emitter, Manager, State};

#[tauri::command]
pub fn upload_images(
    app: tauri::AppHandle,
    db: State<BkDb>,
    paths: Vec<String>,
    prompt: Option<String>,
) -> Result<ImportBatchResult, AppError> {
    let conn = db.0.lock().map_err(|e| AppError::Message(e.to_string()))?;
    let prompt = prompt
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let mut results = Vec::new();
    let mut errors = Vec::new();
    for path in &paths {
        match image_service::import(&conn, &app, path) {
            Ok((image, is_duplicate)) => {
                if let Some(content) = &prompt {
                    if let Err(e) = image_service::relate_prompt(&conn, &image.id, content) {
                        errors.push(image_service::ImportError {
                            path: path.clone(),
                            message: format!("关联提示词失败: {e}"),
                        });
                    }
                }
                results.push(ImportResult {
                    image,
                    is_duplicate,
                });
            }
            Err(e) => errors.push(image_service::ImportError {
                path: path.clone(),
                message: e.to_string(),
            }),
        }
    }
    Ok(ImportBatchResult { results, errors })
}

/// 为上传弹窗提供源图预览缩略图：解码源图生成居中缩略图，写入 data 目录（已在 asset scope 内）。
#[tauri::command]
pub fn get_source_thumbnail(app: tauri::AppHandle, source: String) -> Result<String, AppError> {
    use std::path::PathBuf;
    let src = PathBuf::from(&source);
    if !src.is_file() {
        return Err("源文件不存在".into());
    }
    let img = image::open(&src).map_err(|e| AppError::Message(format!("无法读取图像: {e}")))?;
    let thumb = image_service::make_center_thumb(&img)
        .map_err(|e| AppError::Message(format!("生成缩略图失败: {e}")))?;

    let prev_dir = crate::db::data_dir(&app).join("preview");
    std::fs::create_dir_all(&prev_dir).map_err(|e| AppError::Message(e.to_string()))?;
    // 以源文件路径哈希命名，重复选择复用
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    use std::hash::Hasher;
    hasher.write(source.as_bytes());
    let name = format!("pre_{:016x}.jpg", hasher.finish());
    let dest = prev_dir.join(&name);

    if !dest.exists() {
        thumb
            .save(&dest)
            .map_err(|e| AppError::Message(format!("保存预览失败: {e}")))?;
    }
    Ok(dest.to_string_lossy().to_string())
}

#[tauri::command]
pub fn upload_image(
    app: tauri::AppHandle,
    db: State<BkDb>,
    path: String,
) -> Result<ImportResult, AppError> {
    let conn = db.0.lock().map_err(|e| AppError::Message(e.to_string()))?;
    let (image, is_duplicate) =
        image_service::import(&conn, &app, &path).map_err(|e| AppError::Message(e.to_string()))?;
    Ok(ImportResult {
        image,
        is_duplicate,
    })
}

#[tauri::command]
pub fn list_images(db: State<BkDb>) -> Result<Vec<Image>, AppError> {
    let conn = db.0.lock().map_err(|e| AppError::Message(e.to_string()))?;
    image_service::list(&conn).map_err(|e| AppError::Message(e.to_string()))
}

/// 将一批已存在的图像关联到指定提示词（幂等，不重新导入文件），供详情页「从图像列表导入」。
#[tauri::command]
pub fn relate_images_to_prompt(
    db: State<BkDb>,
    prompt_id: String,
    image_ids: Vec<String>,
) -> Result<usize, AppError> {
    let conn = db.0.lock().map_err(|e| AppError::Message(e.to_string()))?;
    let mut count = 0usize;
    for id in &image_ids {
        count += image_service::relate_image_to_prompt(&conn, &prompt_id, id)
            .map_err(|e| AppError::Message(e.to_string()))? as usize;
    }
    Ok(count)
}

#[tauri::command]
pub fn list_trash(db: State<BkDb>) -> Result<Vec<Image>, AppError> {
    let conn = db.0.lock().map_err(|e| AppError::Message(e.to_string()))?;
    image_service::list_trashed(&conn).map_err(|e| AppError::Message(e.to_string()))
}

#[tauri::command]
pub fn delete_image(db: State<BkDb>, id: String) -> Result<Image, AppError> {
    let conn = db.0.lock().map_err(|e| AppError::Message(e.to_string()))?;
    image_service::soft_delete(&conn, &id)
        .map_err(|e| AppError::Message(e.to_string()))?
        .ok_or_else(|| "图像不存在".into())
}

#[tauri::command]
pub fn restore_image(db: State<BkDb>, id: String) -> Result<Image, AppError> {
    let conn = db.0.lock().map_err(|e| AppError::Message(e.to_string()))?;
    image_service::restore(&conn, &id)
        .map_err(|e| AppError::Message(e.to_string()))?
        .ok_or_else(|| "图像不存在".into())
}

#[tauri::command]
pub fn purge_image(app: tauri::AppHandle, db: State<BkDb>, id: String) -> Result<(), AppError> {
    let conn = db.0.lock().map_err(|e| AppError::Message(e.to_string()))?;
    image_service::purge(&conn, &app, &id).map_err(|e| AppError::Message(e.to_string()))
}

/// 恢复全部回收站图像，返回恢复数量。
#[tauri::command]
pub fn restore_all_images(db: State<BkDb>) -> Result<usize, AppError> {
    let conn = db.0.lock().map_err(|e| AppError::Message(e.to_string()))?;
    image_service::restore_all(&conn).map_err(|e| AppError::Message(e.to_string()))
}

/// 清空图像回收站（逐项彻底删除，含磁盘文件），逐项容错。
#[tauri::command]
pub fn empty_image_trash(
    app: tauri::AppHandle,
    db: State<BkDb>,
) -> Result<image_service::TrashBatchResult, AppError> {
    let conn = db.0.lock().map_err(|e| AppError::Message(e.to_string()))?;
    Ok(image_service::empty_trash(&conn, &app))
}

/// 返回指定图像的缩略图磁盘路径，前端配合 convertFileSrc 加载。
#[tauri::command]
pub fn get_thumbnail(
    app: tauri::AppHandle,
    db: State<BkDb>,
    id: String,
) -> Result<String, AppError> {
    let conn = db.0.lock().map_err(|e| AppError::Message(e.to_string()))?;
    let rel: Option<String> = conn
        .query_row(
            "SELECT thumbnail_path FROM images WHERE id = ?1",
            rusqlite::params![id],
            |r| r.get(0),
        )
        .map_err(|e| AppError::Message(e.to_string()))?;

    let Some(rel) = rel else {
        return Err("缩略图不存在".into());
    };
    Ok(crate::db::data_dir(&app)
        .join(&rel)
        .to_string_lossy()
        .into_owned())
}

/// 返回单张图像详情。
#[tauri::command]
pub fn get_image_detail(db: State<BkDb>, id: String) -> Result<Image, AppError> {
    let conn = db.0.lock().map_err(|e| AppError::Message(e.to_string()))?;
    image_service::get_by_id(&conn, &id)
        .map_err(|e| AppError::Message(e.to_string()))?
        .ok_or_else(|| "图像不存在".into())
}

/// 返回图像原图磁盘路径，前端配合 convertFileSrc 加载（详情页大图使用）。
#[tauri::command]
pub fn get_image_src(
    app: tauri::AppHandle,
    db: State<BkDb>,
    id: String,
) -> Result<String, AppError> {
    let conn = db.0.lock().map_err(|e| AppError::Message(e.to_string()))?;
    let rel: Option<String> = conn
        .query_row(
            "SELECT relative_path FROM images WHERE id = ?1",
            rusqlite::params![id],
            |r| r.get(0),
        )
        .map_err(|e| AppError::Message(e.to_string()))?;

    let Some(rel) = rel else {
        return Err("原图不存在".into());
    };
    Ok(crate::db::data_dir(&app)
        .join(&rel)
        .to_string_lossy()
        .into_owned())
}

/// 更新图像详情字段（文件名、备注、收藏、安全评级）。
#[tauri::command]
pub fn update_image_detail(
    db: State<BkDb>,
    id: String,
    file_name: Option<String>,
    note: Option<String>,
    is_favorite: Option<bool>,
    is_safe: Option<bool>,
) -> Result<Image, AppError> {
    let conn = db.0.lock().map_err(|e| AppError::Message(e.to_string()))?;
    image_service::update_detail(
        &conn,
        &id,
        file_name.as_deref(),
        note.as_deref(),
        is_favorite,
        is_safe,
    )
    .map_err(|e| AppError::Message(e.to_string()))?
    .ok_or_else(|| "图像不存在".into())
}

/// 返回图像的标签列表。
#[tauri::command]
pub fn get_image_tags(db: State<BkDb>, id: String) -> Result<Vec<ImageTag>, AppError> {
    let conn = db.0.lock().map_err(|e| AppError::Message(e.to_string()))?;
    let mut stmt = conn
        .prepare(
            "SELECT it.id, it.name
             FROM image_tags it
             JOIN image_tag_relations itr ON itr.tag_id = it.id
             WHERE itr.image_id = ?1
             ORDER BY it.name",
        )
        .map_err(|e| AppError::Message(e.to_string()))?;
    let rows = stmt
        .query_map(rusqlite::params![id], |r| {
            Ok(ImageTag {
                id: r.get(0)?,
                name: r.get(1)?,
                group_id: None,
            })
        })
        .map_err(|e| AppError::Message(e.to_string()))?;
    let mut tags = Vec::new();
    for row in rows {
        tags.push(row.map_err(|e| AppError::Message(e.to_string()))?);
    }
    Ok(tags)
}

/// 为图像添加多个标签：标签不存在则创建，关联存在则忽略，并更新图像的 updated_at。
#[tauri::command]
pub fn add_image_tags(
    db: State<BkDb>,
    id: String,
    names: Vec<String>,
) -> Result<Vec<ImageTag>, AppError> {
    let conn = db.0.lock().map_err(|e| AppError::Message(e.to_string()))?;
    let tx = conn
        .unchecked_transaction()
        .map_err(|e| AppError::Message(e.to_string()))?;

    let mut result = Vec::new();
    for raw in names {
        let name = raw.trim();
        if name.is_empty() {
            continue;
        }
        // 获取或创建标签
        let tag_id: i64 = match tx
            .query_row(
                "SELECT id FROM image_tags WHERE name = ?1",
                rusqlite::params![name],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| AppError::Message(e.to_string()))?
        {
            Some(tid) => tid,
            None => {
                tx.execute(
                    "INSERT INTO image_tags(name) VALUES (?1)",
                    rusqlite::params![name],
                )
                .map_err(|e| AppError::Message(e.to_string()))?;
                tx.last_insert_rowid()
            }
        };
        // 建立关联（重复时因主键冲突报错，忽略）
        let _ = tx.execute(
            "INSERT OR IGNORE INTO image_tag_relations(image_id, tag_id) VALUES (?1, ?2)",
            rusqlite::params![id, tag_id],
        );
        result.push(ImageTag {
            id: tag_id,
            name: name.to_string(),
            group_id: None,
        });
    }

    tx.execute(
        "UPDATE images SET updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id = ?1",
        rusqlite::params![id],
    )
    .map_err(|e| AppError::Message(e.to_string()))?;
    tx.commit().map_err(|e| AppError::Message(e.to_string()))?;
    Ok(result)
}

/// 移除图像的一个标签关联。
#[tauri::command]
pub fn remove_image_tag(db: State<BkDb>, id: String, tag_id: i64) -> Result<(), AppError> {
    let conn = db.0.lock().map_err(|e| AppError::Message(e.to_string()))?;
    conn.execute(
        "DELETE FROM image_tag_relations WHERE image_id = ?1 AND tag_id = ?2",
        rusqlite::params![id, tag_id],
    )
    .map_err(|e| AppError::Message(e.to_string()))?;
    Ok(())
}

/// 返回全部图像标签（供标签筛选区渲染），按名称排序。
#[tauri::command]
pub fn list_all_image_tags(db: State<BkDb>) -> Result<Vec<ImageTag>, AppError> {
    let conn = db.0.lock().map_err(|e| AppError::Message(e.to_string()))?;
    let mut stmt = conn
        .prepare("SELECT id, name, group_id FROM image_tags ORDER BY name")
        .map_err(|e| AppError::Message(e.to_string()))?;
    let rows = stmt
        .query_map([], |r| {
            Ok(ImageTag {
                id: r.get(0)?,
                name: r.get(1)?,
                group_id: r.get(2)?,
            })
        })
        .map_err(|e| AppError::Message(e.to_string()))?;
    let mut tags = Vec::new();
    for row in rows {
        tags.push(row.map_err(|e| AppError::Message(e.to_string()))?);
    }
    Ok(tags)
}

/// 返回非删除图像到其标签名的映射：{imageId: [tagName,...]}，供前端内存过滤。
#[tauri::command]
pub fn get_image_tags_map(
    db: State<BkDb>,
) -> Result<std::collections::HashMap<String, Vec<String>>, AppError> {
    let conn = db.0.lock().map_err(|e| AppError::Message(e.to_string()))?;
    let mut stmt = conn
        .prepare(
            "SELECT img.id, it.name
             FROM images img
             JOIN image_tag_relations itr ON itr.image_id = img.id
             JOIN image_tags it ON it.id = itr.tag_id
             WHERE img.is_deleted = 0
             ORDER BY it.name",
        )
        .map_err(|e| AppError::Message(e.to_string()))?;
    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
        .map_err(|e| AppError::Message(e.to_string()))?;
    let mut map: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
    for row in rows {
        let (img_id, tag_name) = row.map_err(|e| AppError::Message(e.to_string()))?;
        map.entry(img_id).or_default().push(tag_name);
    }
    Ok(map)
}

/// 返回非删除图像到其关联提示词内容的映射：{imageId: [content,...]}，供卡片 row2 显示。
#[tauri::command]
pub fn get_image_prompts_map(
    db: State<BkDb>,
) -> Result<std::collections::HashMap<String, Vec<String>>, AppError> {
    let conn = db.0.lock().map_err(|e| AppError::Message(e.to_string()))?;
    let mut stmt = conn
        .prepare(
            "SELECT img.id, pr.content
             FROM images img
             JOIN prompt_image_relations pir ON pir.image_id = img.id
             JOIN prompts pr ON pr.id = pir.prompt_id
             WHERE img.is_deleted = 0 AND pr.is_deleted = 0
             ORDER BY pr.created_at",
        )
        .map_err(|e| AppError::Message(e.to_string()))?;
    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
        .map_err(|e| AppError::Message(e.to_string()))?;
    let mut map: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
    for row in rows {
        let (img_id, content) = row.map_err(|e| AppError::Message(e.to_string()))?;
        map.entry(img_id).or_default().push(content);
    }
    Ok(map)
}

/// 返回单张图像关联的提示词列表（含标题/内容/翻译/备注/标签），供详情页左侧展示。
#[tauri::command]
pub fn get_image_related_prompts(
    db: State<BkDb>,
    id: String,
) -> Result<Vec<LinkedPrompt>, AppError> {
    let conn = db.0.lock().map_err(|e| AppError::Message(e.to_string()))?;
    let mut stmt = conn
        .prepare(
            "SELECT pr.id, pr.title, pr.content, pr.content_translate, pr.note
             FROM prompt_image_relations pir
             JOIN prompts pr ON pr.id = pir.prompt_id
             WHERE pir.image_id = ?1 AND pr.is_deleted = 0
             ORDER BY pr.created_at",
        )
        .map_err(|e| AppError::Message(e.to_string()))?;
    let rows = stmt
        .query_map(rusqlite::params![id], |r| {
            Ok(LinkedPrompt {
                id: r.get(0)?,
                title: r.get(1)?,
                content: r.get(2)?,
                content_translate: r.get(3)?,
                note: r.get(4)?,
                tags: Vec::new(),
            })
        })
        .map_err(|e| AppError::Message(e.to_string()))?;
    let mut list: Vec<LinkedPrompt> = rows
        .collect::<Result<_, _>>()
        .map_err(|e| AppError::Message(e.to_string()))?;
    // 补充分组查询每条提示词的标签
    for p in list.iter_mut() {
        let mut t = conn
            .prepare(
                "SELECT pt.name
                 FROM prompt_tag_relations ptr
                 JOIN prompt_tags pt ON pt.id = ptr.tag_id
                 WHERE ptr.prompt_id = ?1
                 ORDER BY pt.name",
            )
            .map_err(|e| AppError::Message(e.to_string()))?;
        let names = t
            .query_map(rusqlite::params![p.id], |r| r.get::<_, String>(0))
            .map_err(|e| AppError::Message(e.to_string()))?;
        p.tags = names
            .collect::<Result<_, _>>()
            .map_err(|e| AppError::Message(e.to_string()))?;
    }
    Ok(list)
}

/// 为指定图像新建提示词并关联（复用 create_prompt + relate），供图像详情「新建提示词」。
#[tauri::command]
pub fn create_prompt_for_image(
    db: State<BkDb>,
    content: String,
    image_id: String,
) -> Result<(), AppError> {
    let conn = db.0.lock().map_err(|e| AppError::Message(e.to_string()))?;
    let prompt = prompt_service::create(&conn, &content, None)
        .map_err(|e| AppError::Message(e.to_string()))?;
    image_service::relate_image_to_prompt(&conn, &prompt.id, &image_id)
        .map_err(|e| AppError::Message(e.to_string()))?;
    Ok(())
}

/// 设置页「重建缩略图」：扫描全部图像，补齐丢失的缩略图文件并回写路径，
/// 进度经 thumbnail-rebuild-progress 事件推送。重 IO 长任务，async + spawn_blocking。
#[tauri::command]
pub async fn rebuild_thumbnails(app: tauri::AppHandle) -> Result<RebuildSummary, AppError> {
    tauri::async_runtime::spawn_blocking(move || {
        let data_dir = crate::db::data_dir(&app);
        let thumbs_root = crate::db::thumbnails_dir(&app);
        let bk = app.state::<BkDb>();
        let conn = bk.0.lock().map_err(|e| e.to_string())?;
        thumbnail_service::rebuild_all(&data_dir, &thumbs_root, &conn, |done, total, file_name| {
            let _ = app.emit(
                thumbnail_service::PROGRESS_EVENT,
                RebuildProgress {
                    current: done,
                    total,
                    file_name: file_name.to_string(),
                },
            );
        })
    })
    .await
    .map_err(|e| AppError::Message(format!("重建缩略图任务执行失败: {e}")))?
    .map_err(AppError::from)
}

/// 懒自愈：批量校验指定图像的缩略图文件，缺失且原图存在时按需生成并回写。
/// 正常路径仅 N 次文件存在性检查，同步命令即可。
#[tauri::command]
pub fn ensure_image_thumbnails(
    app: tauri::AppHandle,
    db: State<BkDb>,
    ids: Vec<String>,
) -> Result<EnsureResult, AppError> {
    let conn = db.0.lock().map_err(|e| AppError::Message(e.to_string()))?;
    thumbnail_service::ensure(
        &crate::db::data_dir(&app),
        &crate::db::thumbnails_dir(&app),
        &conn,
        &ids,
    )
    .map_err(AppError::from)
}
