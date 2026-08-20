//! 标签 Tauri commands：薄适配层，从 managed state 取连接，转调领域服务。

use crate::db::BkDb;
use crate::features::tag::service;

use tauri::State;

#[tauri::command]
pub fn list_tags(db: State<BkDb>) -> Result<Vec<service::Tag>, String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    service::list(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn create_tag(db: State<BkDb>, name: String) -> Result<service::Tag, String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    service::create(&conn, &name).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_tag(db: State<BkDb>, id: i64) -> Result<(), String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    service::remove(&conn, id).map_err(|e| e.to_string())
}