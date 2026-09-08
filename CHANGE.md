# 变更日志

按版本记录有影响的改动（架构/重构/修复）。日常环境要点与踩坑见 [AGENTS.md](AGENTS.md)。

## v0.2.18

### 新增：paim 自有全量备份导出/导入

- 备份包结构（对齐 pm）：`manifest.json + database/paim.db + files/images/**`；缩略图不导出，导入端重建。
- 后端：新增 `domain/paim_backup_service.rs(+test)` 与 `commands/paim_backup.rs`（`inspect/export/import_paim_backup` 三命令）。导出用 `VACUUM INTO` 生成库快照（原子、自动合并 WAL，不断库）+ 图像目录复制 + ZipWriter 压缩；导入与 pm 同构——内存占位连接换绑 → 数据目录整目录让位（`paim-data_{时间戳}`）→ 解包换库文件 → `db::init` 重开（自动迁移旧版本备份）→ 重建缩略图 → 失败自动回滚。校验：manifest 存在、`appName == "paim"`、`dataVersion ≤ 1`。
- 重构：ZIP 条目读取/安全路径/临时目录/递归复制/`open_app_db` 等从 `pm_backup_service` 抽到 `domain/backup_common.rs`，pm 与 paim 备份服务共用。备份对外类型同轨合一：`BackupInfo`/`BackupExportSummary`/`BackupImportSummary`/`BackupProgress`（事件统一为 `backup-progress`），净减 3 个重复类型；`BackupManifest`/校验规则保持各自独立（appName 区分来源）。pm 概览补 `trashed_prompt_count`。
- 导入入口合一：后端 `detect_app` 按 manifest appName 探测来源（`paim`/`pm`）分发，命令合并为 `inspect_backup`/`import_backup`/`export_backup` 三条（`BackupInfo` 增 `app` 字段）；前端两套导入流程合并为一按钮一确认一弹窗，确认文案标注识别出的来源。
- 测试：`paim_backup_service.test.rs` 覆盖导出→恢复往返（逐表一致、图像文件落位、包内条目齐备）、manifest 校验（appName/版本）、缺库文件报错。
- 压缩体验优化：压缩阶段逐条目量化进度（80→99，含文件名 detail）；jpg/jpeg/png/webp/gif 按 Stored 直存（deflate 对已压缩格式收益 <1% 却耗 CPU），manifest/db 维持 deflate；大文件改 BufReader 流式写入避免整读进内存。

## v0.2.17

### 新增：侧栏「统计」弹窗（对齐 pm 统计页）

- 入口：左侧栏底部按钮组新增「统计」（柱状图图标，位于「刷新缓存」上方），点击弹出全局统计弹窗，每次打开实时查询（与 pm 行为一致，无缓存）。
- 内容：左右两栏各 6 项，与 pm 统计弹窗一一对应——提示词（总数/已删除/已收藏/含图像/标签组数/标签总数）、图像（总数/已删除/已收藏/有引用/标签组数/标签总数）。
- 口径与 pm 一致：已收藏只数在册项；「含图像」= 有关联图像（且图像未删除）的活跃提示词；「有引用」= 被活跃提示词关联的未删除图像（EXISTS 双向过滤回收站）。两处刻意差异：paim 无全局安全模式，不做 `isSafeOnly` 过滤；标签总数按 `COUNT(*)` 含未分组标签（pm 只数组内标签）。
- 后端：新增 `domain/statistics_service.rs(+test)`（SQL 聚合 + EXISTS，12 项一次返回）与 `commands/stats.rs::get_statistics`，`lib.rs` 注册；`bindings.ts` 由 `pnpm check` 重生。
- 前端：新增 `src/components/StatsModal.vue`（Teleport 弹窗，关闭按钮 + 点遮罩关闭，风格对齐 ConfirmDialog）；`App.vue` 侧栏加按钮与开关。
- 测试：`statistics_service` 单测覆盖 12 项口径（回收站收藏不计、双向引用过滤、未分组标签计入）与空库全 0。

### 新增：侧栏「信息」卡片信息开关（对齐 pm）

- 入口：左侧栏底部按钮组「刷新缓存」与「统计」之间新增信息按钮（眼睛图标）。显示态常态灰色，隐藏态加深底色，title 动态「显示/隐藏卡片信息」。
- 行为与 pm 一致：关闭后卡片仅剩背景图/占位图与悬浮按钮行，隐藏 row2 内容预览、row3 标签、row4 排序字段；row1 悬浮按钮保留。
- 状态：新增 `src/utils/cardInfo.ts`——模块级 ref + localStorage `cardInfoVisible`（默认显示），提示词页/图像页/侧栏全局共享，KeepAlive 切页即时同步。
- 刻意差异：pm 用 `Ctrl+I`，与 paim 的「切换图像页」冲突，改用 `Alt+I`（App.vue 全局监听，侧栏按钮 title 同步标注）。

## v0.2.16

### 修复：提示词/图像主页按更新时间排序错乱

- 现象：提示词主页「更新时间」排序结果与实际不符——默认「最新在前」时反而把 4 月老数据排在 9 月新数据之前。卡片显示的时间是对的，但顺序错。
- 根因：`updated_at` 在库里并存两种格式——paim 原生/pm 新版为 ISO 8601 UTC（`2026-09-08T04:13:54.348Z`），pm 早期备份为本地斜杠（`2026/4/15 22:40:24`）。排序比较（`list_prompts` 的 `ORDER BY updated_at` 与前端 `localeCompare`）都是字符串序，而 `'-'`(45) < `'/'`(47)，两类数据被切成两个互不可比区间，跨格式排序即错乱。
- 后端（治本）：导入时把 `prompts`/`images` 的 `created_at`/`updated_at`/`deleted_at` 经 `normalize_ts` 统一规整为 ISO 8601 UTC（斜杠本地时间按本地墙钟转 UTC，显示时间不变；ISO 原样规整为 `Z` 毫秒格式），不再整表 `INSERT...SELECT` 原样搬入。导入后库内时间字段格式一致，SQL `ORDER BY` 与前端的字符串/时间戳排序都正确。时间规整函数下沉到 `infra/time.rs`（纯函数，供各处复用）。`deleted_at` 同样规整：两个回收站列表（`ORDER BY deleted_at DESC`）依赖它排序。
- 前端（加固）：时间类排序统一改用 `utils/date.ts::toTimestamp`（转数值时间戳比较，跨格式正确）：`PromptPage.vue`、`ImagePage.vue`（updatedAt/createdAt）、`ImagePickerModal.vue`（updatedAt/createdAt）三处，杜绝未来再混入非规范格式时复发。其余排序依据（title/fileName 字符串、fileSize/width/height 数值、标签 name/sort_order/count）与时间格式无关，无影响。
- 测试：`infra/time.test.rs` 新增 `normalize_ts_canonicalizes_iso_and_slash`（ISO 原样规整、斜杠转合法 RFC3339 且以 `Z` 结尾、无法识别保留原值）；`pm_backup_service.test.rs` 种子数据的回收站 `deleted_at` 改为斜杠格式，集成断言导入后被规整为 ISO。

### 修复：软删除/恢复提示词或图像时刷新关联对方 updated_at

- 现状：`purge`/`empty_trash`（级联解绑）已刷关联对方 `updated_at`，但软删除与恢复路径漏了。而回收站中的提示词/图像会从对方的关联列表消失（`is_deleted` 过滤），语义同为隐式解绑/重挂，对方的「更新时间」排序应感知。
- 改动（单事务 + 子查询，软删除不清 relation 行故无需先收集 id）：
  - `prompt_service::remove`/`restore`：刷关联图像 `WHERE id IN (SELECT image_id FROM prompt_image_relations WHERE prompt_id=?)`；
  - `prompt_service::restore_all`：先刷后恢复——只刷「回收站提示词的关联图像」，避免恢复后无法区分刚恢复者而误刷无关图像；
  - `image_service::soft_delete`/`restore`/`restore_all`：镜像对称。
- 前端无需改动：删/恢复/清空各入口已 `markPageStale` 对侧页面。
- 测试：两侧各新增 `soft_delete_restore_restore_all_touch_related_prompts(/images)`，覆盖软删/恢复刷新 + `restore_all` 不误刷在册关联。

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
