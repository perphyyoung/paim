pub mod db;
pub mod features;
pub mod logging;

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

      // 启动时清空导入预览目录，避免长期积累（需要时由命令按需重建）
      let _ = std::fs::remove_dir_all(db::data_dir(app.handle()).join("preview"));

      Ok(())
    })
    .invoke_handler(tauri::generate_handler![
      // —— 提示词通用 ——
      features::prompt::list_prompts,
      features::prompt::create_prompt,
      features::prompt::delete_prompt,
      features::prompt::update_prompt_title,
      features::prompt::update_prompt_detail,
      features::prompt::create_prompt_with_images,
      features::prompt::add_images_to_prompt,
      features::prompt::list_trashed_prompts,
      features::prompt::restore_prompt,
      features::prompt::purge_prompt,
      features::prompt::get_prompt_tags_map,
      features::prompt::get_prompt_images_count_map,
      features::prompt::get_prompt_thumbs_map,
      features::prompt::get_prompt_related_images,
      features::prompt::get_prompt_tag_data,
      features::prompt::add_prompt_tags,
      features::prompt::remove_prompt_tag,
      features::prompt::remove_prompt_image,      
      // —— 图像通用 ——
      features::image::upload_image,
      features::image::upload_images,
      features::image::get_source_thumbnail,
      features::image::list_images,
      features::image::get_thumbnail,
      features::image::get_image_detail,
      features::image::get_image_src,
      features::image::update_image_detail,
      features::image::create_prompt_for_image,
      features::image::relate_images_to_prompt,
      features::image::list_trash,
      features::image::delete_image,
      features::image::restore_image,
      features::image::purge_image,
      features::image::get_image_tags,
      features::image::add_image_tags,
      features::image::remove_image_tag,
      features::image::list_all_image_tags,
      features::image::get_image_tags_map,
      features::image::get_image_prompts_map,
      features::image::get_image_related_prompts,
      // —— 提示词标签管理 ——
      features::prompt_tag::list_prompt_tag_groups,
      features::prompt_tag::create_prompt_tag_group,
      features::prompt_tag::update_prompt_tag_group,
      features::prompt_tag::delete_prompt_tag_group,
      features::prompt_tag::create_prompt_tag,
      features::prompt_tag::rename_prompt_tag,
      features::prompt_tag::delete_prompt_tag,
      features::prompt_tag::move_prompt_tag_to_group,
      features::prompt_tag::pin_prompt_tag_group_to_top,
      // —— 图像标签管理 ——
      features::image_tag::list_image_tag_groups,
      features::image_tag::create_image_tag_group,
      features::image_tag::update_image_tag_group,
      features::image_tag::delete_image_tag_group,
      features::image_tag::create_image_tag,
      features::image_tag::rename_image_tag,
      features::image_tag::delete_image_tag,
      features::image_tag::move_tag_to_group,
      features::image_tag::pin_image_tag_group_to_top,
      // —— 日志 & 数据目录 ——
      logging::log_msg,
      db::get_data_dir,
      db::open_data_dir,
    ])
    .run(tauri::generate_context!())
    .expect("error while running tauri application");
}
