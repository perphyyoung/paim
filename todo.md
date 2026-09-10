# todo

## 缓存优化

现状与规划见 [缓存及加载优化设计.md](缓存及加载优化设计.md)：§7 为各缓存容量速查，§8 为规划中未实施；本文件只放**待办与已落地记录**（本文件是临时性文件，其它文件不得引用它）。

### 已落地（2026-09-10）

- **P0 实体级缓存 LRU 封顶**（`src/utils/entityCache.ts`）：`createEntityCache(load, maxEntries)`，Map 保插入序，命中即 `delete+set` 移到队尾，超限从队首（最久未用）淘汰。上限：`relatedImagesCache` / `relatedPromptsCache` 100 条（单条 1–10 KB → 约 1 MB 封顶），`imageSrcCache` / `imageTagsCache` 300 条。
  - 正确性：淘汰只降命中率，未命中即重新读库；失效仍走既有 `invalidate` 白名单。
  - 未加 `clear()`：导入、重置、快捷键一律走 `location.reload()`，模块级缓存随之释放，加了就是死代码。
  - 验证：`pnpm format:ui` + `pnpm typecheck` 通过；`pnpm e2e` 全绿（2026-09-10）。
- **P1 回收站随开随弃**：两主页 `closeTrash()` 清空集合；`ImagePage.loadTrash()` 复用已缓存的 `dataDir`（为空才 `getDataDir()`），少一次 IPC。

### 待办

- **P2 主页全量映射窗口化分页**（触发条件：数据集 >2 万条 或 全量加载 >300 ms）

  前提——分页之所以重，是因为**排序 / 搜索 / 标签筛选 / 特殊标签计数目前全在前端**，拿到全量才算得出来（`ImagePage.vue:77-225`、`PromptPage.vue` 同构）：

  - 排序：6 键（createdAt / updatedAt / fileSize / fileName / width / height）× 升降序，`sortedImages` 内存排序，其中 fileName 排的是 `stored_name`。
  - 搜索：`matchesKeyword(file_name, note, 标签名)`，输入即过滤，**无防抖**。
  - 标签筛选：多标签 AND + 反选 `invertedTagFilter` + 特殊标签（favorite / unreferenced / multiRef / noTag / safe / unsafe，依赖 `tagNames`、`imagePrompts`）。
  - 角标：`specialCounts` 对全量 `images.value.filter` 计数；筛选区标签 count 来自后端 `getTagData`（只统计未删除，与筛选条件无关，可不动）。
  - 批量：`selectAll` / `invert` 基于当前列表。
  - 后端现状：`image_service::list(search, tag, limit)` 只支持**单 tag**、固定 `ORDER BY created_at DESC`、**无 offset**；`prompt_service::list()` 无任何参数；`image_service::count` 已有。

  S1 后端（可独立上线，UX 无变化）：

  - `list_images` / `list_prompts` 扩为 `(offset, limit, search, tags[], inverted, special[], sort, desc)` → `{items, total}`。
  - 排序白名单映射：createdAt→`created_at`、updatedAt→`updated_at`、fileSize→`file_size`、fileName→`stored_name`、width/height→`COALESCE(width,0)`；方向由 `desc` 拼 `ASC/DESC`（禁止字符串拼接用户输入）。
  - search 沿用现有 `filter_sql`（已覆盖 file_name / note / 标签名，含 `ESCAPE '\'`）；多标签改为每个标签一个 `EXISTS(...)`，反选整体包 `NOT(...)`。
  - 特殊标签落 SQL：favorite→`is_favorite=1`、safe/unsafe→`is_safe`、noTag→`NOT EXISTS` 标签关系、unreferenced/multiRef→`(SELECT COUNT(*) FROM prompt_image_relations WHERE image_id=?) = 0 / > 1`（注意是否要滤掉 `is_deleted` 的提示词，与前端 `imagePrompts` 口径对齐）。
  - 新增 `list_*_ids(...)`（同条件只回 id，供「全选 / 反选 / 批量」用）与 `special_counts(...)`（6 个 COUNT 一次回，替代前端全量 filter）。
  - 索引配套：排序键 `file_size` / `stored_name` / `width` / `height` 大数据集下要索引，否则 `ORDER BY` 走 filesort（现有索引只有 is_deleted / updated_at / title 系列，见 `infra/db.rs:133-155`）。
  - 命令签名变化由 `pnpm check` 自动重生 `src/bindings.ts`。

  S2 前端：

  - `usePagedBlocks`：`Map<blockIndex, Item[]>`，块大小取「`gridPageSize` × 2」；可见区间换算所需块，缺块才拉，`seqId` 防竞态；未加载块用占位对象填（`{ id, __placeholder: true }`），保证 VirtualGrid 的 totalHeight 与定位不变（总条数取后端 `total`）。
  - 条件变化（keyword / tags / sort / inverted）→ 清块缓存 + 取 total + 拉首屏；**keyword 必须加 300 ms 防抖**（当前是即时过滤）。
  - `selectAll` / `invert` 走 `list_*_ids`；`specialCounts` 走 `special_counts` 命令。
  - 缩略图懒自愈 `visibleIds` 跳过占位项，逻辑不变。
  - 滚动恢复：KeepAlive 的 `restoreSaved()` 需等目标块加载完再定位，否则会跳位。

  S3 兼容与回退：

  - 阈值开关（建议 2 万条）：低于阈值仍走全量，e2e 与日常使用不受影响；分页模式另加专项用例（现有 19 个用例按全量 DOM 编写，分批后需滚动才拿得到卡片）。

  对用户体验的影响（关键取舍）：

  | 场景               | 现在（全量）                | 分页后                       | 影响               |
  | ------------------ | --------------------------- | ---------------------------- | ------------------ |
  | 首屏打开           | 一次拉全部（大集 >1 s）     | 只拉首屏                     | 正（仅大集可见）   |
  | 搜索输入           | 即时过滤、零延迟、无 loading | 300 ms 防抖 + 每次 IPC       | **负（最明显）**   |
  | 排序切换           | 瞬时（内存 sort/reverse）   | 一次加载 + 可能闪骨架        | **负**             |
  | 标签筛选点选       | 瞬时                        | 一次加载（本机约几十~几百 ms） | 负（轻微）         |
  | 滚动到未加载区     | 无                          | 占位骨架 / 空洞              | **负**（快拖可见） |
  | 特殊标签计数       | 瞬时                        | 随列表同批返回               | 中性               |
  | 全选 / 反选        | 瞬时                        | 需一次 id 查询 + loading     | 中性               |
  | 内存占用           | 全量常驻                    | 只驻留已看块                 | 正                 |
  | 交互失败面         | 只在首次加载                | 每次交互都可能失败/需错误态  | 负                 |

  建议顺序（性价比）：**P3 字段投影 → Web Worker 托管全量 → 真分页**。投影把 `gen_params` / `stored_name` 等详情才用的字段挪出常驻内存（注意 `note` 参与卡片搜索不能去），UX 零影响；Worker 把排序/筛选搬出主线程，仍即时、无 IPC 延迟，主线程堆下降但总内存不降；真分页只对「内存绝对量」有效，代价是上表那几项交互延迟，留到确实上万条时再做。

- **P3 主页字段瘦身（备选，见上「建议顺序」）**：卡片只保留轻字段投影，`gen_params` / `stored_name` 等详情才用的字段按需取；注意 `note` 参与卡片前端搜索（`ImagePage.vue` 的 `matchesKeyword`），不能从投影里去掉。

### 验证方式

- 功能回归：`pnpm e2e`（2026-09-10 已全绿）。
- 内存判据：DevTools Memory 快照对比「翻遍全部详情前后」的 heap 增量——P0 落地后应从单调增长变为封顶在 `maxEntries × 单条体积`。
- 分页专项（做 P2 时）：全量/分页双模式各跑一遍，重点看搜索防抖、排序切换、快拖滚动三处的观感与竞态。
