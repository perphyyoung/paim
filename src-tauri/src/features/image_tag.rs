//! 图像标签管理命令：薄适配层，转调通用标签管理模块（manager）。
//! 数据源为 image_tags / image_tag_groups / image_tag_relations 表。

use crate::db::BkDb;
use crate::error::AppError;
use crate::features::tag_manager;
use crate::features::tag_manager::TagDomain;
use tauri::State;

/// 返回图像标签管理页所需数据（标签组 + 带计数的标签）。
#[tauri::command]
pub fn list_image_tag_groups(db: State<BkDb>) -> Result<tag_manager::TagManagerData, AppError> {
    let conn = db.0.lock().map_err(|e| AppError::Message(e.to_string()))?;
    tag_manager::load_manager_data(&conn, TagDomain::Image)
        .map_err(|e| AppError::Message(e.to_string()))
}

/// 新建标签组，返回新组。
#[tauri::command]
pub fn create_image_tag_group(
    db: State<BkDb>,
    name: String,
    sort_order: Option<i64>,
) -> Result<tag_manager::TagGroup, AppError> {
    if name.trim().is_empty() {
        return Err("组名不能为空".into());
    }
    let conn = db.0.lock().map_err(|e| AppError::Message(e.to_string()))?;
    tag_manager::create_group(&conn, TagDomain::Image, &name, sort_order)
        .map_err(|e| AppError::Message(e.to_string()))
}

/// 编辑标签组：更新名称与排序数值。
#[tauri::command]
pub fn update_image_tag_group(
    db: State<BkDb>,
    id: i64,
    name: String,
    sort_order: Option<i64>,
) -> Result<(), AppError> {
    if name.trim().is_empty() {
        return Err("组名不能为空".into());
    }
    let conn = db.0.lock().map_err(|e| AppError::Message(e.to_string()))?;
    tag_manager::update_group(&conn, TagDomain::Image, id, &name, sort_order)
        .map_err(|e| AppError::Message(e.to_string()))
}

/// 删除标签组（组内标签交由外键 ON DELETE SET NULL 变为未分组）。
#[tauri::command]
pub fn delete_image_tag_group(db: State<BkDb>, id: i64) -> Result<(), AppError> {
    let conn = db.0.lock().map_err(|e| AppError::Message(e.to_string()))?;
    tag_manager::delete_group(&conn, TagDomain::Image, id)
        .map_err(|e| AppError::Message(e.to_string()))
}

/// 新建标签（可指定所属组），返回新标签。
#[tauri::command]
pub fn create_image_tag(
    db: State<BkDb>,
    name: String,
    group_id: Option<i64>,
) -> Result<tag_manager::TagItem, AppError> {
    if name.trim().is_empty() {
        return Err("标签名不能为空".into());
    }
    let conn = db.0.lock().map_err(|e| AppError::Message(e.to_string()))?;
    tag_manager::create_tag(&conn, TagDomain::Image, &name, group_id)
        .map_err(|e| AppError::Message(e.to_string()))
}

/// 重命名标签。
#[tauri::command]
pub fn rename_image_tag(db: State<BkDb>, id: i64, name: String) -> Result<(), AppError> {
    if name.trim().is_empty() {
        return Err("标签名不能为空".into());
    }
    let conn = db.0.lock().map_err(|e| AppError::Message(e.to_string()))?;
    tag_manager::rename_tag(&conn, TagDomain::Image, id, &name)
        .map_err(|e| AppError::Message(e.to_string()))
}

/// 删除标签（关联关系由外键 CASCADE 一并清除）。
#[tauri::command]
pub fn delete_image_tag(db: State<BkDb>, id: i64) -> Result<(), AppError> {
    let conn = db.0.lock().map_err(|e| AppError::Message(e.to_string()))?;
    tag_manager::delete_tag(&conn, TagDomain::Image, id)
        .map_err(|e| AppError::Message(e.to_string()))
}

/// 移动标签到指定组（group_id 为 null 表示未分组）。
#[tauri::command]
pub fn move_tag_to_group(db: State<BkDb>, id: i64, group_id: Option<i64>) -> Result<(), AppError> {
    let conn = db.0.lock().map_err(|e| AppError::Message(e.to_string()))?;
    tag_manager::move_tag(&conn, TagDomain::Image, id, group_id)
        .map_err(|e| AppError::Message(e.to_string()))
}

/// 将标签组固定到首位（sort_order 设为当前最小值 - 1）。
#[tauri::command]
pub fn pin_image_tag_group_to_top(db: State<BkDb>, id: i64) -> Result<(), AppError> {
    let conn = db.0.lock().map_err(|e| AppError::Message(e.to_string()))?;
    tag_manager::pin_group_to_top(&conn, TagDomain::Image, id)
        .map_err(|e| AppError::Message(e.to_string()))
}
