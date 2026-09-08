# todo

## 进行中

### 1. 添加标签入口支持标签自动完成（对齐 pm）

**目标**：三处「添加标签」输入框支持候选下拉；候选不区分图像/提示词域，两域合并去重后共用。

**现状入口（3 个）**

| # | 入口 | 文件 | 现有实现 | 候选数据 |
|---|---|---|---|---|
| 1 | 图像详情·图像标签 | `ImageDetailModal.vue:764` | 裸 input + 添加按钮（`useTagAdd`） | 无，需惰性拉取 |
| 2 | 提示词详情·提示词标签 | `PromptDetailModal.vue:742` | 同上 | `props.allTags`（仅本域，不够用） |
| 3 | 批量添加标签弹窗 | `BatchActionBar.vue:139`（两主页共用） | 裸 input + 确定/取消，emit `add-tag` | 组件通用、无域信息 |

**新增文件**

1. `src/features/tag/useTagCandidates.ts` — 单例候选仓库
   - `tagCandidates: Ref<string[]>`：合并去重后的候选名（对外只读）
   - `ensureTagCandidates()`：惰性加载，`Promise` 去重防并发；只拉「尚未合并过的域」
   - `mergeTagNames(domain, names)`：主页 `loadTagFilter()` 后并入本域已有数据，零额外请求
   - `addTagName(name)`：添加成功后本地补名
   - `invalidateTagCandidates()`：标签管理保存 / 导入恢复后清空
   - 内部 `mergedDomains: Set<"image" | "prompt">`，两域齐了才算完整
   - 数据源：`getTagData("image").tags` + `getTagData("prompt").tags`；trim 后精确去重，按 `localeCompare("zh")` 升序

2. `src/features/tag/components/TagAutocompleteInput.vue`
   - props：`modelValue`、`candidates`、`exclude?`、`placeholder`、`max`(20)、`loading?`
   - `Teleport to body` + `fixed` 定位（详情弹窗内是滚动容器，绝对定位会被裁），层级 `z-[125]`（> 顶层模态 z-[120]，< toast z-[130]）
   - 事件：`select(name)`（点击候选 / 回车命中高亮）、`submit(name)`（回车无高亮，提交原始输入）
   - 行为对齐 pm：空输入不弹；大小写不敏感前缀匹配；高亮匹配片段；↑↓ 导航；Enter 选中后立即提交；Esc 关下拉并 `stopPropagation`；点击外部关闭；候选项用 `mousedown + preventDefault` 处理，避免 blur 抢先
   - 异步结果带「输入值已变则丢弃」守卫

**改造点**

- `ImageDetailModal.vue`：input → `TagAutocompleteInput`；`candidates = tagCandidates`，聚焦时 `ensureTagCandidates()`；`exclude = tags.map(t => t.name)`；选中即 `addTag`（**不加 `all-tags` prop**，本域数据不够）
- `PromptDetailModal.vue`：同上；`props.allTags` 仍只用于 `loadTags` 还原 id，不参与候选
- `BatchActionBar.vue`：新增可选 prop `suggestions?: string[]`（通用组件不依赖业务）；`ImagePage.vue`、`PromptPage.vue` 传 `:suggestions="tagCandidates"`；选中候选 → `emit("add-tag", name)`（父级流程不变，失败弹窗保持打开）
- 批量场景不传 `exclude`（多条目标签状态不一致）

**刷新时机**：主页 `loadTagFilter()` 后 `mergeTagNames` + 后台 `ensureTagCandidates()` 补齐另一域；详情/批量聚焦输入框时 ensure；添加成功后 `addTagName`；标签管理 `@saved` 后 invalidate 重建。

**明确不做**：「标签管理 → 新建标签」不加自动完成；保持单次只添加一个标签、`isSpecialTag` 校验不变。

**回归清单**：详情添加 / 重复标签提示；批量添加成功与失败弹窗保持；Esc 仍能关详情；下拉不被 toast 与顶层模态遮挡；拖拽打标签、KeepAlive 跨页切换不受影响；提示词详情嵌套打开图像详情时候选正常。

---

### 2. 标签命令重构：图像域 / 提示词域合一（阶段 1 + 阶段 2）

**背景（不对称现状）**

| 能力 | 图像域 | 提示词域 | 对称 |
|---|---|---|---|
| 全量标签 | `list_all_image_tags` → `Vec<ImageTag>`（无 count） | `get_prompt_tag_data` → `{groups, tags[count]}` | ❌ |
| 标签组 | `list_image_tag_groups` | `list_prompt_tag_groups` + `get_prompt_tag_data` 重复返回 | ❌ |
| 单条目标签 | `get_image_tags(id)` | 无（前端用 map 反查） | ❌ |
| 全量映射 | `get_image_tags_map` | `get_prompt_tags_map` | 重复 SQL |
| 添加标签 | `add_image_tag` → `Vec<ImageTag>` | `add_prompt_tag` → `Vec<PromptTagItem>`（假值 group_id/count） | ❌ |
| 标签管理 CRUD | `commands/image_tag.rs` → `tag_manager` | `commands/prompt_tag.rs` → `tag_manager` | ✅ 已对称 |

根因：写侧（标签管理）已按 `TagDomain` 参数化统一，读侧仍散落在 `commands/image.rs` / `commands/prompt.rs` 手写 SQL；类型就近定义长出 `ImageTag` / `PromptTagItem` / `TagItem` 三套。

**已定决策**

- 阶段 1（读）+ 阶段 2（写/管）**一起做**
- `count` 语义统一为 **仅统计未删除条目**（SQL 加 `is_deleted = 0`，修复现 count 含回收站的问题）
- 前端**统一吃后端 count**，`ImagePage` 删除自算 `tagCounts` 的逻辑
- 命令全部带 `domain: "image" | "prompt"` 参数，bindings 类型合一

**新命令清单（`src-tauri/src/commands/tag.rs`，14 个）**

| 命令 | 取代 |
|---|---|
| `get_tag_data(domain)` → `TagData` | `get_prompt_tag_data`、`list_all_image_tags`、`list_image_tag_groups`、`list_prompt_tag_groups` |
| `get_tags_map(domain)` → `Record<string, string[]>` | `get_image_tags_map`、`get_prompt_tags_map` |
| `get_item_tags(domain, id)` → `TagLite[]` | `get_image_tags`（并补齐提示词侧） |
| `add_tag(domain, id, name)` → `TagLite[]` | `add_image_tag`、`add_prompt_tag` |
| `batch_add_tag(domain, ids, name)` | `batch_add_image_tag`、`batch_add_prompt_tag` |
| `remove_tag(domain, id, tag_id)` | `remove_image_tag`、`remove_prompt_tag` |
| `create_tag_group(domain, name, sort_order)` | `create_image_tag_group`、`create_prompt_tag_group` |
| `update_tag_group(domain, id, name, sort_order)` | `update_*_tag_group` |
| `delete_tag_group(domain, id)` | `delete_*_tag_group` |
| `create_tag(domain, name, group_id)` | `create_image_tag`、`create_prompt_tag` |
| `rename_tag(domain, id, name)` | `rename_image_tag`、`rename_prompt_tag` |
| `delete_tag(domain, id)` | `delete_image_tag`、`delete_prompt_tag` |
| `move_tag_to_group(domain, id, group_id)` | `move_*_tag_to_group` |
| `pin_tag_group_to_top(domain, id)` | `pin_*_tag_group_to_top` |

**类型合一**

- `TagLite { id, name }`（新增）：单条目标签列表、增删结果
- `TagData { groups: TagGroup[], tags: TagItem[] }`（由 `TagManagerData` 改名，筛选区与标签管理页共用）
- 删除 `ImageTag`（`domain/image_service.rs`）、`PromptTagItem` / `PromptTagData` / `PromptTagGroup`（`commands/prompt.rs`）

**实施步骤**

1. [x] `domain/tag_manager.rs`：`TagDomain` 增 `items_table()`；`TagManagerData` → `TagData`、`load_manager_data` → `load_tag_data`；count 改为仅统计未删除；新增 `TagLite`
2. [x] `domain/tag_service.rs`（新建）：`load_tag_data` / `load_tags_map` / `load_item_tags` / `add_tag` / `batch_add_tag` / `remove_tag`，按域参数化
3. [x] `commands/tag.rs`（新建）：上表 14 个命令，薄适配 + 空名校验
4. [x] 清理旧代码：删 `commands/image_tag.rs`、`commands/prompt_tag.rs`；删 `image.rs` 6 个标签命令、`prompt.rs` 5 个标签命令及本地类型；删 `image_service` / `prompt_service` 中标签函数与 `ImageTag`
5. [x] 更新 `domain.rs` / `commands.rs` 模块声明、`lib.rs` 注册
6. [x] 前端：`ImagePage`（3 命令 → 1，吃后端 count）、`PromptPage`、`ImageDetailModal`、`PromptDetailModal`、`ImagePickerModal`、`TagManagerModal`、`useTagAdd`、`useBatchTagAdd`、`useTagDragToCard`、`detailCache`
7. [x] 校验：`pnpm check` 通过（bindings 已重生，`TagDomain` 正确生成为 `"image" | "prompt"`）；vue-tsc 通过；标签测试迁至 `domain/tag_service.test.rs`（原两 service 测试中的标签用例移除，新增 count/ map 忽略回收站的用例），待 `pnpm test` 复验

**行为不变项**：软删过滤、单事务批量、添加/移除标签后刷自身 `updated_at`、关联不存在时报错（不静默建孤立关联）。
