pub mod db;
pub mod error;
pub mod features;
pub mod logging;
pub mod text_utils;

use serde::{Deserialize, Serialize};
use tauri::Manager;
use tauri_specta::Event;

/// 全局快捷键触发事件（payload 为动作名，如 "toggle-settings"）。
#[derive(Debug, Serialize, Deserialize, Clone, specta::Type, tauri_specta::Event)]
#[tauri_specta(event_name = "global-shortcut")]
pub struct GlobalShortcutEvent(pub String);

/// tauri-specta 命令注册表：单一事实源，同时供 invoke_handler 与 TS 绑定导出使用。
/// 新增命令必须：① `#[specta::specta]` 标注；② 在此注册；③ 跑 debug 构建（pnpm dev）
/// 自动重新导出 ../src/bindings.ts（见下方 export_bindings，流程详见 docs/新增命令说明(tauri-specta 版).md）。
fn specta_builder() -> tauri_specta::Builder<tauri::Wry> {
    tauri_specta::Builder::<tauri::Wry>::new()
        // 官方推荐：将 i64/u64 等 BigInt 类型统一导出为 TS number（file_size 值域 < 2^53，安全）
        .dangerously_cast_bigints_to_number()
        // 错误以 Promise reject 抛出（bindings 返回 Promise<T>），与前端现有 try/catch + toast 模式一致
        .error_handling(tauri_specta::ErrorHandlingMode::Throw)
        .events(tauri_specta::collect_events![
            GlobalShortcutEvent,
            features::thumbnail_service::ThumbnailRebuildProgress,
            features::pm_backup_service::PmImportProgress,
        ])
        .commands(tauri_specta::collect_commands![
            // —— 提示词通用 ——
            features::prompt::list_prompts,
            features::prompt::create_prompt,
            features::prompt::delete_prompt,
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
            features::prompt::set_prompt_first_image,
            features::prompt::get_prompt_tag_data,
            features::prompt::add_prompt_tag,
            features::prompt::batch_add_prompt_tag,
            features::prompt::remove_prompt_tag,
            features::prompt::remove_image_from_prompt,
            features::image::remove_prompt_from_image,
            // —— 图像通用 ——
            features::image::select_images,
            features::image::import_images,
            features::image::get_source_thumbnail,
            features::image::list_images,
            features::image::get_image_thumbnail,
            features::image::get_image_detail,
            features::image::get_image_src,
            features::image::replace_image,
            features::image::update_image_detail,
            features::image::create_prompt_for_image,
            features::image::relate_images_to_prompt,
            features::image::list_trashed_images,
            features::image::delete_image,
            features::image::restore_image,
            features::image::purge_image,
            features::image::restore_all_images,
            features::image::empty_image_trash,
            features::image::get_image_tags,
            features::image::add_image_tag,
            features::image::batch_add_image_tag,
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
            features::image_tag::move_image_tag_to_group,
            features::image_tag::pin_image_tag_group_to_top,
            // —— 日志 & 数据目录 ——
            logging::log_msg,
            db::get_data_dir,
            db::open_data_dir,
            db::open_image_location,
            db::batch_toggle_image_favorite,
            db::batch_toggle_prompt_favorite,
            features::prompt::sync_prompt_safe_to_images,
            features::image::sync_image_safe_to_prompts,
            // —— pm 备份导入 ——
            features::pm_backup::inspect_pm_backup,
            features::pm_backup::import_pm_backup,
        ])
}

/// 导出 TypeScript 绑定：debug 构建启动时自动导出到 ../src/bindings.ts
/// （测试二进制在 Windows 下有 DLL 加载问题，故不在测试中导出）。
#[cfg(debug_assertions)]
fn export_bindings(builder: &tauri_specta::Builder<tauri::Wry>) {
    builder
        .export(
            specta_typescript::Typescript::default(),
            "../src/bindings.ts",
        )
        .expect("导出 TypeScript 绑定失败");
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let specta_builder = specta_builder();

    #[cfg(debug_assertions)]
    export_bindings(&specta_builder);

    tauri::Builder::default()
    .invoke_handler(specta_builder.invoke_handler())
    .setup(move |app| {
      specta_builder.mount_events(app);
      app.handle().plugin(tauri_plugin_dialog::init())?;

      // 全局快捷键：Ctrl+Shift+, 切换设置面板（系统级钩子，不受输入法/WebView 焦点影响）
      // 注：原 Ctrl+, 已被系统其它程序注册为全局热键，插件无法抢占，故加 Shift
      #[cfg(desktop)]
      {
        use tauri_plugin_global_shortcut::{Code, Modifiers, Shortcut, ShortcutState};
        let toggle_settings = Shortcut::new(
          Some(Modifiers::CONTROL | Modifiers::SHIFT),
          Code::Comma,
        );
        app.handle().plugin(
          tauri_plugin_global_shortcut::Builder::new()
            .with_shortcuts([toggle_settings])?
            .with_handler(move |app, shortcut, event| {
              if shortcut == &toggle_settings && event.state() == ShortcutState::Pressed {
                let _ = GlobalShortcutEvent("toggle-settings".into()).emit(app);
              }
            })
            .build(),
        )?;
      }

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

      // 启动时清空临时目录（预览图/备份解压/e2e 测试数据），避免长期积累
      let _ = std::fs::remove_dir_all(db::temp_dir(app.handle()));

      Ok(())
    })
    .run(tauri::generate_context!())
    .expect("error while running tauri application");
}
