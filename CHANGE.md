# 变更日志

按版本记录有影响的改动（架构/重构/修复）。日常环境要点与踩坑见 [AGENTS.md](AGENTS.md)。

## v0.2.14

架构规范专项：引入分层规则与结构检查，清理既有违例。

### 新增

- 分层规则 [`.sentrux/rules.toml`](.sentrux/rules.toml)：Rust 三层（infra / domain / commands）+ Web 五层（bindings / shared / features / views / app）+ e2e，共 8 个 order；另设 3 条点名禁令：`components/** ✗ features/**`、`bindings.ts ✗ src/**`、`src/** ✗ e2e/**`。由 `sentrux check .` 强制执行。
- `项目架构.md`：目录结构、分层与依赖方向、关键链路与质量门的说明入口。

### 重构与修复

- **前端分层归位**（原违例：共享层依赖业务切片）
  - `useThumbnailSelfHeal` 从 `src/components/` 移入 `src/features/image/`（业务行为回到业务切片）；
  - `TagChip`（纯 props 标签胶囊）与 `CardTagRow`（string 标签截断行）上移 `src/components/`，解掉 `MediaCard.vue → features/image/components` 的反向依赖。

- **后端抽出 `features/image_ops.rs`**：`open_image` 解码、`make_center_thumb`（200×200 居中裁剪）、`THUMB_SIZE`。
  - 消除 `image` 同名歧义造成的**假环**：外部图像处理 crate `image` 与本地命令模块 `features::image` 同名，sentrux 将 `thumbnail_service.rs` 的 `image::open` 按名归并成对本地 `image.rs` 的依赖，凭空造出 `image.rs ↔ image_service.rs ↔ thumbnail_service.rs` 回边。源码核实三文件间本无回边。
  - 全项目对 `image::` 的调用收拢为唯一入口；顺带实现**缩略图职责单源化**——此前 `image_service`（导入时内联生成）与 `thumbnail_service`（重建/补缺）各有一份裁剪实现，现共用 `image_ops`；`pm_backup_service → thumbnail_service::rebuild_all` 保持单向合法。

### 结果

- `sentrux check .` 全部规则通过，无遗留待办。

### 遗留约定

- 外部 crate `image` 与 `features::image` 同名：任何图像处理调用统一经 `image_ops.rs`，不要在其它模块直接写 `image::`，以免再次触发同名歧义误判。
