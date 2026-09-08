// 分层目录即层名：commands(2) → domain(1) → infra(0)，由 .sentrux/rules.toml 按目录强制
pub mod commands;
pub mod domain;
pub mod infra;

use crate::infra::{db, logging};
use serde::{Deserialize, Serialize};
use tauri::Manager;
use tauri_specta::Event;

/// 全局快捷键触发事件（payload 为动作名，如 "toggle-settings"）。
#[derive(Debug, Serialize, Deserialize, Clone, specta::Type, tauri_specta::Event)]
#[tauri_specta(event_name = "global-shortcut")]
pub struct GlobalShortcutEvent(pub String);

/// tauri-specta 命令注册表：单一事实源，同时供 invoke_handler 与 TS 绑定导出使用。
/// 新增命令必须：① `#[specta::specta]` 标注；② 在此注册；③ 跑 debug 构建（pnpm dev）
/// 自动重新导出 ../src/bindings.ts（见下方 export_bindings，流程详见 docs/新增命令说明(tauri-specta 版).md）；
/// bindings 由 pnpm check 直接复写：主程序 PAIM_EXPORT_BINDINGS 导出即退模式
/// （scripts/check-bindings.mjs 调用）。
fn specta_builder() -> tauri_specta::Builder<tauri::Wry> {
    tauri_specta::Builder::<tauri::Wry>::new()
        // 官方推荐：将 i64/u64 等 BigInt 类型统一导出为 TS number（file_size 值域 < 2^53，安全）
        .dangerously_cast_bigints_to_number()
        // 错误以 Promise reject 抛出（bindings 返回 Promise<T>），与前端现有 try/catch + toast 模式一致
        .error_handling(tauri_specta::ErrorHandlingMode::Throw)
        .events(tauri_specta::collect_events![
            GlobalShortcutEvent,
            domain::thumbnail_service::ThumbnailRebuildProgress,
            domain::pm_backup_service::PmImportProgress,
        ])
        .commands(tauri_specta::collect_commands![
            // —— 提示词通用 ——
            commands::prompt::list_prompts,
            commands::prompt::create_prompt,
            commands::prompt::delete_prompt,
            commands::prompt::update_prompt_detail,
            commands::prompt::create_prompt_with_images,
            commands::prompt::add_images_to_prompt,
            commands::prompt::list_trashed_prompts,
            commands::prompt::restore_prompt,
            commands::prompt::purge_prompt,
            commands::prompt::restore_all_prompts,
            commands::prompt::empty_prompt_trash,
            commands::prompt::get_prompt_tags_map,
            commands::prompt::get_prompt_images_count_map,
            commands::prompt::get_prompt_thumbs_map,
            commands::prompt::get_prompt_related_images,
            commands::prompt::set_prompt_first_image,
            commands::prompt::get_prompt_tag_data,
            commands::prompt::add_prompt_tag,
            commands::prompt::batch_add_prompt_tag,
            commands::prompt::remove_prompt_tag,
            commands::prompt::remove_image_from_prompt,
            commands::image::remove_prompt_from_image,
            // —— 图像通用 ——
            commands::image::select_images,
            commands::image::import_images,
            commands::image::get_source_thumbnail,
            commands::image::list_images,
            commands::image::get_image_detail,
            commands::image::get_image_src,
            commands::image::replace_image,
            commands::image::update_image_detail,
            commands::image::create_prompt_for_image,
            commands::image::relate_images_to_prompt,
            commands::image::list_trashed_images,
            commands::image::delete_image,
            commands::image::restore_image,
            commands::image::purge_image,
            commands::image::restore_all_images,
            commands::image::empty_image_trash,
            commands::image::get_image_tags,
            commands::image::add_image_tag,
            commands::image::batch_add_image_tag,
            commands::image::remove_image_tag,
            commands::image::list_all_image_tags,
            commands::image::get_image_tags_map,
            commands::image::get_image_prompts_map,
            commands::image::get_image_related_prompts,
            commands::image::rebuild_thumbnails,
            commands::image::ensure_image_thumbnails,
            // —— 提示词标签管理 ——
            commands::prompt_tag::list_prompt_tag_groups,
            commands::prompt_tag::create_prompt_tag_group,
            commands::prompt_tag::update_prompt_tag_group,
            commands::prompt_tag::delete_prompt_tag_group,
            commands::prompt_tag::create_prompt_tag,
            commands::prompt_tag::rename_prompt_tag,
            commands::prompt_tag::delete_prompt_tag,
            commands::prompt_tag::move_prompt_tag_to_group,
            commands::prompt_tag::pin_prompt_tag_group_to_top,
            // —— 图像标签管理 ——
            commands::image_tag::list_image_tag_groups,
            commands::image_tag::create_image_tag_group,
            commands::image_tag::update_image_tag_group,
            commands::image_tag::delete_image_tag_group,
            commands::image_tag::create_image_tag,
            commands::image_tag::rename_image_tag,
            commands::image_tag::delete_image_tag,
            commands::image_tag::move_image_tag_to_group,
            commands::image_tag::pin_image_tag_group_to_top,
            // —— 日志 & 数据目录 ——
            logging::log_msg,
            db::get_data_dir,
            db::open_data_dir,
            db::open_image_location,
            db::batch_toggle_image_favorite,
            db::batch_toggle_prompt_favorite,
            commands::prompt::sync_prompt_safe_to_images,
            commands::image::sync_image_safe_to_prompts,
            // —— pm 备份导入 ——
            commands::pm_backup::inspect_pm_backup,
            commands::pm_backup::import_pm_backup,
            // —— 数据统计 ——
            commands::stats::get_statistics,
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

/// 供 pnpm check 复写 bindings：以环境变量 PAIM_EXPORT_BINDINGS 触发「导出即退」
/// 模式（见 run() 开头短路），直接重新生成 src/bindings.ts。
/// 路径锚定 CARGO_MANIFEST_DIR（编译期绝对路径），与进程工作目录无关。
#[cfg(debug_assertions)]
fn export_bindings_standalone(builder: &tauri_specta::Builder<tauri::Wry>) {
    let out = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../src/bindings.ts");
    builder
        .export(specta_typescript::Typescript::default(), &out)
        .expect("导出 TypeScript 绑定失败");
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let specta_builder = specta_builder();

    // 导出即退模式：pnpm check 直接复写 src/bindings.ts（等价于 debug 启动自动导出，
    // 但不创建窗口、不连 webview、不初始化任何子系统），导出后立即退出。
    #[cfg(debug_assertions)]
    if std::env::var_os("PAIM_EXPORT_BINDINGS").is_some() {
        export_bindings_standalone(&specta_builder);
        return;
    }

    #[cfg(debug_assertions)]
    export_bindings(&specta_builder);

    tauri::Builder::default()
    .invoke_handler(specta_builder.invoke_handler())
    .setup(move |app| {
      specta_builder.mount_events(app);
      app.handle().plugin(tauri_plugin_dialog::init())?;

      // 全局快捷键：Ctrl+Shift+, 切换设置面板（系统级钩子，不受输入法/WebView 焦点影响）
      // 注：原 Ctrl+, 已被系统其它程序注册为全局热键，插件无法抢占，故加 Shift。
      // e2e 实例（设置了 PAIM_DATA_DIR）跳过注册：全局热键是系统级单例资源，
      // 注册冲突会让第二个实例启动即失败；e2e 也不依赖全局快捷键。
      #[cfg(desktop)]
      if std::env::var("PAIM_DATA_DIR").is_err() {
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
      // 临时目录在数据目录之外（导入让位改名要求分离），但上传预览图也要经 asset 协议加载
      app.asset_protocol_scope().allow_directory(db::temp_dir(app.handle()), true)?;

      // 启动时清空临时目录（预览图/备份解压），避免长期积累；
      // 跳过带实例标识的前缀（e2e-*、wv2-*、preview-*，各实例自行管理生命周期）；
      // 随后清空本实例的上传预览目录，保证每次启动预览全新
      db::clean_temp_dir(app.handle());
      let _ = std::fs::remove_dir_all(db::preview_dir(app.handle()));

      Ok(())
    })
    .run(tauri::generate_context!())
    .expect("error while running tauri application");
}
