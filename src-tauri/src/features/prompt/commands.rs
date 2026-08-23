//! 提示词 Tauri commands：薄适配层，从 managed state 取连接，转调领域服务。

use crate::db::BkDb;
use crate::features::prompt::service;

use serde::Serialize;
use tauri::State;

#[tauri::command]
pub fn list_prompts(db: State<BkDb>) -> Result<Vec<service::Prompt>, String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    service::list(&conn).map_err(|e| e.to_string())
}

/// 返回非删除提示词到其标签名的映射：{promptId: [tagName,...]}，供卡片 row3 与筛选。
#[tauri::command]
pub fn get_prompt_tags_map(
    db: State<BkDb>,
) -> Result<std::collections::HashMap<String, Vec<String>>, String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(
            "SELECT pr.id, pt.name
             FROM prompts pr
             JOIN prompt_tag_relations ptr ON ptr.prompt_id = pr.id
             JOIN prompt_tags pt ON pt.id = ptr.tag_id
             WHERE pr.is_deleted = 0
             ORDER BY pt.name",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
        .map_err(|e| e.to_string())?;
    let mut map: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    for row in rows {
        let (pid, tag_name) = row.map_err(|e| e.to_string())?;
        map.entry(pid).or_default().push(tag_name);
    }
    Ok(map)
}

/// 返回每个提示词关联（未删除）的图像数：{promptId: count}，供「有图」特殊标签与排序。
#[tauri::command]
pub fn get_prompt_images_count_map(
    db: State<BkDb>,
) -> Result<std::collections::HashMap<String, i64>, String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(
            "SELECT pir.prompt_id, COUNT(*)
             FROM prompt_image_relations pir
             JOIN images img ON img.id = pir.image_id
             WHERE img.is_deleted = 0
             GROUP BY pir.prompt_id",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))
        .map_err(|e| e.to_string())?;
    let mut map: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
    for row in rows {
        let (pid, count) = row.map_err(|e| e.to_string())?;
        map.insert(pid, count);
    }
    Ok(map)
}

#[derive(Debug, Serialize, Clone)]
pub struct CreatePromptWithImagesResult {
    pub prompt: service::Prompt,
    pub results: Vec<crate::features::image::ImportResult>,
    pub errors: Vec<crate::features::image::ImportError>,
}

#[derive(Debug, Serialize, Clone)]
pub struct PromptTagItem {
    pub id: i64,
    pub name: String,
    pub group_id: Option<i64>,
    pub count: i64,
}

#[derive(Debug, Serialize, Clone)]
pub struct PromptTagGroup {
    pub id: i64,
    pub name: String,
    pub sort_order: i64,
}

#[derive(Debug, Serialize, Clone)]
pub struct PromptTagData {
    pub groups: Vec<PromptTagGroup>,
    pub tags: Vec<PromptTagItem>,
}

/// 返回提示词标签筛选区所需数据：标签组 + 带关联数的标签。
#[tauri::command]
pub fn get_prompt_tag_data(db: State<BkDb>) -> Result<PromptTagData, String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    let mut groups = Vec::new();
    {
        let mut stmt = conn
            .prepare("SELECT id, name, sort_order FROM prompt_tag_groups ORDER BY sort_order, name")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |r| {
                Ok(PromptTagGroup {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    sort_order: r.get(2)?,
                })
            })
            .map_err(|e| e.to_string())?;
        for row in rows {
            groups.push(row.map_err(|e| e.to_string())?);
        }
    }
    let mut tags = Vec::new();
    {
        let mut stmt = conn
            .prepare(
                "SELECT t.id, t.name, t.group_id, COUNT(r.tag_id) AS cnt
                 FROM prompt_tags t
                 LEFT JOIN prompt_tag_relations r ON r.tag_id = t.id
                 GROUP BY t.id
                 ORDER BY t.name",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |r| {
                Ok(PromptTagItem {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    group_id: r.get(2)?,
                    count: r.get(3)?,
                })
            })
            .map_err(|e| e.to_string())?;
        for row in rows {
            tags.push(row.map_err(|e| e.to_string())?);
        }
    }
    Ok(PromptTagData { groups, tags })
}

/// 新建提示词（内容必需）；image_paths 非空时上传并关联到该提示词。
#[tauri::command]
pub fn create_prompt_with_images(
    db: State<BkDb>,
    app: tauri::AppHandle,
    content: String,
    title: Option<String>,
    image_paths: Vec<String>,
) -> Result<CreatePromptWithImagesResult, String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    let prompt = service::create(&conn, &content, title).map_err(|e| e.to_string())?;
    let mut results = Vec::new();
    let mut errors = Vec::new();
    for path in &image_paths {
        match crate::features::image::import(&conn, &app, path) {
            Ok((image, is_duplicate)) => {
                if let Err(e) = crate::features::image::relate_image_to_prompt(
                    &conn,
                    &prompt.id,
                    &image.id,
                ) {
                    errors.push(crate::features::image::ImportError {
                        path: path.clone(),
                        message: format!("关联图像失败: {e}"),
                    });
                }
                results.push(crate::features::image::ImportResult { image, is_duplicate });
            }
            Err(e) => errors.push(crate::features::image::ImportError {
                path: path.clone(),
                message: e.to_string(),
            }),
        }
    }
    Ok(CreatePromptWithImagesResult {
        prompt,
        results,
        errors,
    })
}

#[tauri::command]
pub fn create_prompt(
    db: State<BkDb>,
    content: String,
    title: Option<String>,
) -> Result<service::Prompt, String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    service::create(&conn, &content, title).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn update_prompt_title(
    db: State<BkDb>,
    id: String,
    title: Option<String>,
) -> Result<service::Prompt, String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    service::update_title(&conn, &id, title).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_prompt(db: State<BkDb>, id: String) -> Result<(), String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    service::remove(&conn, &id).map_err(|e| e.to_string())
}

/// 列出回收站中的提示词（已软删除）。
#[tauri::command]
pub fn list_trashed_prompts(db: State<BkDb>) -> Result<Vec<service::Prompt>, String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    service::list_trashed(&conn).map_err(|e| e.to_string())
}

/// 恢复回收站中的提示词。
#[tauri::command]
pub fn restore_prompt(db: State<BkDb>, id: String) -> Result<service::Prompt, String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    service::restore(&conn, &id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "提示词不存在".to_string())
}

/// 彻底删除回收站中的提示词。
#[tauri::command]
pub fn purge_prompt(db: State<BkDb>, id: String) -> Result<(), String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    service::purge(&conn, &id).map_err(|e| e.to_string())
}

/// 返回每个提示词第一张关联（未删除）图像的缩略图磁盘路径：{promptId: absPath}，供卡片背景。
#[tauri::command]
pub fn get_prompt_thumbs_map(
    app: tauri::AppHandle,
    db: State<BkDb>,
) -> Result<std::collections::HashMap<String, String>, String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(
            "SELECT prompt_id, thumbnail_path
             FROM (
                SELECT pir.prompt_id AS prompt_id, img.thumbnail_path AS thumbnail_path,
                       ROW_NUMBER() OVER (PARTITION BY pir.prompt_id ORDER BY pir.rowid) AS rn
                FROM prompt_image_relations pir
                JOIN images img ON img.id = pir.image_id
                WHERE img.is_deleted = 0
             )
             WHERE rn = 1",
        )
        .map_err(|e| e.to_string())?;
    let data_dir = crate::db::data_dir(&app);
    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
        .map_err(|e| e.to_string())?;
    let mut map: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for row in rows {
        let (pid, thumb_rel) = row.map_err(|e| e.to_string())?;
        map.insert(pid, data_dir.join(&thumb_rel).to_string_lossy().into_owned());
    }
    Ok(map)
}