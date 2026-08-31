pub mod db;
pub mod error;
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

      // 启动防呆：激活数据目录缺失但存在备用数据集
      // 目录时，说明切换未完成——不静默创建空库，提示用户完成改名后退出。
      let pending = db::pending_switch_datasets(app.handle());
      if !pending.is_empty() {
        use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
        let data_dir = db::data_dir(app.handle());
        let dir_name = data_dir
          .file_name()
          .and_then(|n| n.to_str())
          .unwrap_or("paim-data")
          .to_string();
        app
          .handle()
          .dialog()
          .message(format!(
            "未找到数据目录：\n{}\n\n但发现了备用数据集：\n{}\n\n可能是数据集切换未完成。请关闭本提示后将目标数据集目录改名为「{}」，再重新启动应用。",
            data_dir.display(),
            pending.join("\n"),
            dir_name
          ))
          .title("paim")
          .kind(MessageDialogKind::Warning)
          .buttons(MessageDialogButtons::Ok)
          .blocking_show();
        // setup 阶段事件循环尚未运行，且此时未打开数据库，直接退出进程即可
        std::process::exit(0);
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
      features::prompt::restore_all_prompts,
      features::prompt::empty_prompt_trash,
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
      features::image::restore_all_images,
      features::image::empty_image_trash,
      features::image::get_image_tags,
      features::image::add_image_tags,
      features::image::remove_image_tag,
      features::image::list_all_image_tags,
      features::image::get_image_tags_map,
      features::image::get_image_prompts_map,
      features::image::get_image_related_prompts,
      features::image::rebuild_thumbnails,
      features::image::ensure_image_thumbnails,
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
      // —— pm 备份导入 ——
      features::pm_backup::inspect_pm_backup,
      features::pm_backup::import_pm_backup,
    ])
    .run(tauri::generate_context!())
    .expect("error while running tauri application");
}
