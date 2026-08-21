pub mod db;
pub mod features;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
  tauri::Builder::default()
    .setup(|app| {
      app.handle().plugin(tauri_plugin_dialog::init())?;

      if cfg!(debug_assertions) {
        app.handle().plugin(
          tauri_plugin_log::Builder::default()
            .level(log::LevelFilter::Info)
            .build(),
        )?;
      }

      let db_path = db::user_db_path(app.handle());
      if let Some(dir) = db_path.parent() {
        std::fs::create_dir_all(dir)?;
      }
      let db = db::init(db_path)?;
      app.manage(db);

      Ok(())
    })
    .invoke_handler(tauri::generate_handler![
      features::prompt::commands::list_prompts,
      features::prompt::commands::create_prompt,
      features::prompt::commands::update_prompt_title,
      features::prompt::commands::delete_prompt,
      features::tag::commands::list_tags,
      features::tag::commands::create_tag,
      features::tag::commands::delete_tag,
      features::image::import_image,
      features::image::list_images,
    ])
    .run(tauri::generate_context!())
    .expect("error while running tauri application");
}
