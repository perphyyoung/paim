# todo

本文件仅供临时性的进度追踪，其它文件不得引用。

## P2（暂缓，待单独一轮）：主线程 / 单连接锁重构

背景：`update_prompt_detail` 等写命令是同步命令（宿主主线程执行），内部 `db.0.lock()` 无超时；任何长持锁任务都会把窗口连同保存一起冻住（排查见 docs/lessons.md 第 28 节）。本轮只做了观测性（锁等待/持锁 >500ms 记 WARN + 保存路径成对埋点）与 llama 降级，架构层未动。

- [ ] 写命令一律不再占主线程：`update_prompt_detail` 等改为 `#[tauri::command(async)]`（宏的 sync_threadpool），或统一走 `commands::db_blocking`；作废 comments/commands.rs 里「写命令毫秒级可留主线程」的前提（单连接锁上不成立）。
- [ ] 长持锁命令改「短锁取数据 → 无锁做重活 → 短锁回写」：`ensure_prompt_thumbnails` / `ensure_image_thumbnails` / `rebuild_thumbnails` / `import_images` / `create_prompt_with_images` / `add_images_to_prompt`（现状：整批解码+编码在锁内，且是同步命令）。
- [ ] `log_msg` 等文件 IO 命令移出主线程（独立 writer 或 async + spawn_blocking）。
- [ ] 复核 `rank_in` 全表向量扫描持锁时长（万级向量时数百 ms~数秒）。
