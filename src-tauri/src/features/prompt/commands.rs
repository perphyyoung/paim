//! 提示词 Tauri commands：薄适配层，从 managed state 取连接，转调领域服务。

use crate::db::BkDb;
use crate::features::prompt::service;

use tauri::State;

#[tauri::command]
pub fn list_prompts(db: State<BkDb>) -> Result<Vec<service::Prompt>, String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    service::list(&conn).map_err(|e| e.to_string())
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
    id: i64,
    title: Option<String>,
) -> Result<service::Prompt, String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    service::update_title(&conn, id, title).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_prompt(db: State<BkDb>, id: i64) -> Result<(), String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    service::remove(&conn, id).map_err(|e| e.to_string())
}