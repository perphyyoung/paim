# 变更日志

按版本记录有影响的改动（架构/重构/修复）。日常环境要点与踩坑见 [AGENTS.md](AGENTS.md)。

## v0.2.15

### 重构：后端目录即层名

- 删除混排的 `src-tauri/src/features/`（命令层与领域层同目录）与 `features.rs`，改为三个顶层目录：
  - `commands/`（order 2）：`prompt.rs`(+test)、`image.rs`、`prompt_tag.rs`、`image_tag.rs`、`pm_backup.rs`；
  - `domain/`（order 1）：`prompt_service.rs`(+test)、`image_service.rs`(+test)、`image_ops.rs`、`thumbnail_service.rs`(+test)、`tag_manager.rs`、`pm_backup_service.rs`(+test)；
  - `infra/`（order 0）：`db.rs`(+test)、`error.rs`、`logging.rs`、`text_utils.rs`(+test)。
- 层模块声明放在同名的 `commands.rs` / `domain.rs` / `infra.rs`（不用 mod.rs），`lib.rs` 只声明这三个模块。
- 业务切片不再靠后端目录体现，改由文件前缀承担（`prompt` / `image` / `tag` / `pm_backup`）；前端仍是 `src/features/<切片>/`。
- `.sentrux/rules.toml` 三条 glob 简化为整目录匹配（`infra/**`、`domain/**`、`commands/**`）：新文件放错目录会被 `sentrux check .` 拦下；同时修掉此前 `features/prompt.test.rs` 不属于任何层的漏网。
- 影响面：约 107 处 `features::` 引用改为 `commands::` / `domain::`，`crate::{db,error,logging,text_utils}` 改为 `crate::infra::*`；命令函数名不变，`src/bindings.ts` 无变化，前端零改动。
- 验证：`pnpm check` 通过；`pnpm test` 通过；`sentrux check .` 全部规则通过。

### 修复：详情弹窗「未改动也写库」

- 现象：详情弹窗进入编辑态后未改动直接保存，仍会写库；`prompts` 列表按 `updated_at DESC` 排序，导致该记录被顶到最前。
- 前端：`PromptDetailModal.vue` / `ImageDetailModal.vue` 的 `saveFields()` 做**逐字段**脏检查（标题/内容/文件名按 trim 后比对），只把真正变化的字段传给后端，未变化的传 `null`；全部未变则直接退出编辑态并提示「没有改动」，不发起命令。非空校验也保留在前端（标题/内容/文件名必填）。
- 后端：`prompt_service::update_detail` 与 `image_service::update_detail` 只按传入的 `Some` 字段拼**一条** UPDATE，未传的字段不动；去掉了「先读当前行再逐字段比较」的逻辑与后端非空校验，改为「前端判定、后端直写」。全部字段传 `None` 时不写库、`updated_at` 不变。
  - `Some("")` 表示显式清空（翻译/备注允许），与「不更新」的 `None` 语义区分；因此前端判定必须用 `=== null`，不能用真值判断，否则清空备注会被误判为无改动。
- 净效果：提示词空保存 0 次写、0 次读；改动 1 个字段由 5 次 UPDATE 降为 1 次 UPDATE + 1 次回读（省掉比较用的预读）。
- 测试：`prompt_service.test.rs` 以 `update_detail_only_writes_provided_fields`（只传标题则其余字段不变）与 `update_detail_all_none_does_not_write`（全 `None` 不刷新 `updated_at`）取代原 `update_detail_no_change_does_not_write`；删除依赖后端校验的 `update_detail_rejects_empty_title_and_content` 与 `update_detail_rejects_empty_file_name`。

### 优化：详情弹窗切换不再重复读库

- 现象：切换提示词详情条目时，左栏关联图像每次都重新读库；后端取关联数据还在循环里逐条查标签，关联 N 项就是 N+1 次查询。
- 前端：新增实体级缓存 `features/prompt/api/relatedImagesCache.ts`（关联图像）与 `features/image/api/detailCache.ts`（关联提示词 / 原图路径 / 标签），挂在模块作用域——详情弹窗被父级 `v-if` 强制卸载，组件内 ref 存不住。命中缓存直接渲染、不进 loading；切换后预取相邻项，连续翻页几乎全命中。失效只在数据真的变化时做：关联增删、设为首图、替换图像、标签增删、安全评级联动。
- 后端：`prompt_service::list_related_images` 与 `image_service::list_related_prompts` 把标签查询移出循环，统一走 `domain/tag_manager.rs::tags_by_owner`（一次 `IN (...)` 批量查后按 id 分组回填，N+1 条 SQL 降为 2 条）；`get_image_related_prompts` 的查询逻辑随之从命令层下沉到领域层，命令层只留薄壳。
- `list_related_images` 拆出 `list_related_images_with`（数据目录由调用方注入），便于脱离 Tauri 单测。
- 前端两处缓存逻辑抽成 `src/utils/entityCache.ts` 的 `createEntityCache` 工厂（模块级 Map + in-flight 去重），`relatedImagesCache` 与 `detailCache` 的三个缓存只是一行实例化。
- 测试：两侧各补一条「多项关联 + 不同标签」用例，断言标签按名称升序且归属各自条目、不串到邻居。

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

- 外部 crate `image` 与本地模块 `commands::image` 同名（重构后路径，当时为 `features::image`）：任何图像处理调用统一经 `domain/image_ops.rs`，不要在其它模块直接写 `image::`，以免再次触发同名歧义误判。
