//! 标签命令：按域（图像/提示词）合一的薄适配层。
//!
//! 图像标签与提示词标签读写同构，命令统一带 `domain` 参数，
//! 转调 `domain::tag_service`（查询与关联）与 `domain::tag_manager`（标签本体/分组 CRUD）。
//! 取代原先分散在 image.rs / prompt.rs / image_tag.rs / prompt_tag.rs 的四套命令。

use crate::domain::tag_manager::{
    self, TagData, TagDomain, TagGroup, TagItem, TagLite, TagNameKind,
};
use crate::domain::tag_service;
use crate::infra::db::BkDb;
use crate::infra::error::AppError;
use std::collections::HashMap;
use tauri::State;

// ============ 读取 ============

/// 域内全部标签数据（标签组 + 带未删除计数的标签）：筛选区与标签管理页共用。
#[tauri::command]
#[specta::specta]
pub fn get_tag_data(db: State<BkDb>, domain: TagDomain) -> Result<TagData, AppError> {
    let conn = db.0.lock().map_err(|e| AppError::Message(e.to_string()))?;
    tag_service::load_tag_data(&conn, domain).map_err(|e| AppError::Message(e.to_string()))
}

/// 未删除实体到其标签名的映射：{itemId: [tagName,...]}，供列表内存过滤与卡片标签行。
#[tauri::command]
#[specta::specta]
pub fn get_tags_map(
    db: State<BkDb>,
    domain: TagDomain,
) -> Result<HashMap<String, Vec<String>>, AppError> {
    let conn = db.0.lock().map_err(|e| AppError::Message(e.to_string()))?;
    tag_service::load_tags_map(&conn, domain).map_err(|e| AppError::Message(e.to_string()))
}

/// 单个实体的标签列表（按名称升序）。
#[tauri::command]
#[specta::specta]
pub fn get_item_tags(
    db: State<BkDb>,
    domain: TagDomain,
    id: String,
) -> Result<Vec<TagLite>, AppError> {
    let conn = db.0.lock().map_err(|e| AppError::Message(e.to_string()))?;
    tag_service::load_item_tags(&conn, domain, &id).map_err(|e| AppError::Message(e.to_string()))
}

// ============ 关联增删 ============

/// 为单个实体添加一个标签（标签不存在则创建），返回该标签并刷新实体 updated_at。
#[tauri::command]
#[specta::specta]
pub fn add_tag(
    db: State<BkDb>,
    domain: TagDomain,
    id: String,
    name: String,
) -> Result<Vec<TagLite>, AppError> {
    if name.trim().is_empty() {
        return Err("标签名不能为空".into());
    }
    let conn = db.0.lock().map_err(|e| AppError::Message(e.to_string()))?;
    tag_service::add_tag(&conn, domain, &id, &name).map_err(|e| AppError::Message(e.to_string()))
}

/// 为多个实体批量添加同一个标签（单事务），并逐个刷新 updated_at。
#[tauri::command]
#[specta::specta]
pub fn batch_add_tag(
    db: State<BkDb>,
    domain: TagDomain,
    ids: Vec<String>,
    name: String,
) -> Result<(), AppError> {
    if name.trim().is_empty() {
        return Err("标签名不能为空".into());
    }
    let conn = db.0.lock().map_err(|e| AppError::Message(e.to_string()))?;
    let id_refs: Vec<&str> = ids.iter().map(String::as_str).collect();
    tag_service::batch_add_tag(&conn, domain, &id_refs, &name)
        .map_err(|e| AppError::Message(e.to_string()))
}

/// 移除实体的一个标签关联，并刷新实体 updated_at。
#[tauri::command]
#[specta::specta]
pub fn remove_tag(
    db: State<BkDb>,
    domain: TagDomain,
    id: String,
    tag_id: i64,
) -> Result<(), AppError> {
    let conn = db.0.lock().map_err(|e| AppError::Message(e.to_string()))?;
    tag_service::remove_tag(&conn, domain, &id, tag_id)
        .map_err(|e| AppError::Message(e.to_string()))
}

// ============ 标签管理（标签本体与分组） ============

/// 新建标签组，返回新组。
#[tauri::command]
#[specta::specta]
pub fn create_tag_group(
    db: State<BkDb>,
    domain: TagDomain,
    name: String,
    sort_order: Option<i64>,
) -> Result<TagGroup, AppError> {
    if name.trim().is_empty() {
        return Err("组名不能为空".into());
    }
    let conn = db.0.lock().map_err(|e| AppError::Message(e.to_string()))?;
    tag_manager::ensure_name_not_dup(&conn, domain, TagNameKind::Group, &name, None)
        .map_err(AppError::Message)?;
    tag_manager::create_group(&conn, domain, &name, sort_order)
        .map_err(|e| AppError::Message(e.to_string()))
}

/// 编辑标签组：更新名称与排序数值。
#[tauri::command]
#[specta::specta]
pub fn update_tag_group(
    db: State<BkDb>,
    domain: TagDomain,
    id: i64,
    name: String,
    sort_order: Option<i64>,
) -> Result<(), AppError> {
    if name.trim().is_empty() {
        return Err("组名不能为空".into());
    }
    let conn = db.0.lock().map_err(|e| AppError::Message(e.to_string()))?;
    tag_manager::ensure_name_not_dup(&conn, domain, TagNameKind::Group, &name, Some(id))
        .map_err(AppError::Message)?;
    tag_manager::update_group(&conn, domain, id, &name, sort_order)
        .map_err(|e| AppError::Message(e.to_string()))
}

/// 删除标签组（组内标签交由外键 ON DELETE SET NULL 变为未分组）。
#[tauri::command]
#[specta::specta]
pub fn delete_tag_group(db: State<BkDb>, domain: TagDomain, id: i64) -> Result<(), AppError> {
    let conn = db.0.lock().map_err(|e| AppError::Message(e.to_string()))?;
    tag_manager::delete_group(&conn, domain, id).map_err(|e| AppError::Message(e.to_string()))
}

/// 新建标签（可指定所属组），返回新标签。
#[tauri::command]
#[specta::specta]
pub fn create_tag(
    db: State<BkDb>,
    domain: TagDomain,
    name: String,
    group_id: Option<i64>,
) -> Result<TagItem, AppError> {
    if name.trim().is_empty() {
        return Err("标签名不能为空".into());
    }
    let conn = db.0.lock().map_err(|e| AppError::Message(e.to_string()))?;
    tag_manager::ensure_name_not_dup(&conn, domain, TagNameKind::Tag, &name, None)
        .map_err(AppError::Message)?;
    tag_manager::create_tag(&conn, domain, &name, group_id)
        .map_err(|e| AppError::Message(e.to_string()))
}

/// 重命名标签。
#[tauri::command]
#[specta::specta]
pub fn rename_tag(
    db: State<BkDb>,
    domain: TagDomain,
    id: i64,
    name: String,
) -> Result<(), AppError> {
    if name.trim().is_empty() {
        return Err("标签名不能为空".into());
    }
    let conn = db.0.lock().map_err(|e| AppError::Message(e.to_string()))?;
    tag_manager::ensure_name_not_dup(&conn, domain, TagNameKind::Tag, &name, Some(id))
        .map_err(AppError::Message)?;
    tag_manager::rename_tag(&conn, domain, id, &name).map_err(|e| AppError::Message(e.to_string()))
}

/// 删除标签（关联关系由外键 CASCADE 一并清除）。
#[tauri::command]
#[specta::specta]
pub fn delete_tag(db: State<BkDb>, domain: TagDomain, id: i64) -> Result<(), AppError> {
    let conn = db.0.lock().map_err(|e| AppError::Message(e.to_string()))?;
    tag_manager::delete_tag(&conn, domain, id).map_err(|e| AppError::Message(e.to_string()))
}

/// 将标签移动到指定组（group_id 为 null 表示未分组）。
#[tauri::command]
#[specta::specta]
pub fn move_tag_to_group(
    db: State<BkDb>,
    domain: TagDomain,
    id: i64,
    group_id: Option<i64>,
) -> Result<(), AppError> {
    let conn = db.0.lock().map_err(|e| AppError::Message(e.to_string()))?;
    tag_manager::move_tag(&conn, domain, id, group_id).map_err(|e| AppError::Message(e.to_string()))
}

/// 将标签组固定到首位（sort_order 设为当前最小值 - 1）。
#[tauri::command]
#[specta::specta]
pub fn pin_tag_group_to_top(db: State<BkDb>, domain: TagDomain, id: i64) -> Result<(), AppError> {
    let conn = db.0.lock().map_err(|e| AppError::Message(e.to_string()))?;
    tag_manager::pin_group_to_top(&conn, domain, id).map_err(|e| AppError::Message(e.to_string()))
}
