# todo

本文件仅供临时性的进度追踪，其它文件不得引用。

## 已完成：P2 主线程 / 单连接锁重构

- [x] **命令形态收口**：所有访问数据库的命令改为 `async fn` + `infra::db::blocking`（从 `commands::db_blocking` 搬到 `infra/db.rs`，infra 自身的命令也能用）；纯文件 IO / 编解码 / 系统调用改用 `infra/task.rs::spawn_blocking`；只剩微秒级命令（`get_data_dir` / `open_data_dir` / `get_log_level` / `get_webview_dir` / `similarity_index_progress` / `cancel_*` / `image_fullscreen::*` / `e2e_is_window_visible`）保持同步。作废了「写命令毫秒级可留主线程」的旧前提，并记下 `#[tauri::command(async)]` 的陷阱（它占的是 async 工作线程，不是阻塞池）。
- [x] **长持锁命令改三段式**（短锁取数据 → 无锁做重活 → 短锁回写）：
  - 缩略图：`thumbnail_service::targets_by_ids` / `all_targets` / `build_missing` / `write_paths`（懒自愈顺序、全量重建满核）；`prompt_service::thumb_plan` / `thumb_apply`；两个备份导入服务的缩略图重建同口径。
  - 图像导入 / 替换：`prepare_source` / `dedupe_by_md5` / `prepare_files` / `insert_prepared`（并发重复时清理刚落下的文件并复用已有记录）、`replace_ensure_old` / `replace_commit`；三个导入入口共用 `commands::image::import_one`。
  - 检索：`image_rank_candidates` / `prompt_rank_candidates` / `score_ranked`（锁内只取 BLOB，解码 + 点积 + 排序在锁外）。
- [x] `log_msg` / `set_log_level` / `preferences::{export,import}` / `font_family_map::get_font_family_map` / `webview_dir::open_webview_dir` 等文件 IO 命令移出主线程。
- [x] 约定与经验沉淀：`项目架构.md` 新增「命令形态」一节；`docs/开发经验.md` 第 7 节；`docs/lessons.md` 第 12 / 28 节修正与补充；`docs/日志使用说明.md` 措辞同步。
