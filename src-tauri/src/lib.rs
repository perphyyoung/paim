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

      // 将数据目录加入 asset 协议 scope，使前端能通过 convertFileSrc 读取本地图片
      app.asset_protocol_scope().allow_directory(db::data_dir(app.handle()), true)?;

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
      features::image::get_thumbnail,
      features::image::list_trash,
      features::image::delete_image,
      features::image::restore_image,
      features::image::purge_image,
      features::image::get_image_detail,
      features::image::get_image_src,
      features::image::get_image_tags,
      features::image::add_image_tags,
      features::image::remove_image_tag,
      features::image::list_all_image_tags,
      features::image::get_image_tags_map,
      features::image::update_image_detail,
      features::image_tag::list_image_tag_groups,
      features::image_tag::create_image_tag_group,
      features::image_tag::update_image_tag_group,
      features::image_tag::delete_image_tag_group,
      features::image_tag::create_image_tag,
      features::image_tag::rename_image_tag,
      features::image_tag::delete_image_tag,
      features::image_tag::move_tag_to_group,
      features::image_tag::pin_image_tag_group_to_top,
      db::get_data_dir,
      db::open_data_dir,
    ])
    .run(tauri::generate_context!())
    .expect("error while running tauri application");
}
