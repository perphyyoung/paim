# todo

本文件仅供临时性的进度追踪，其它文件不得引用。

## 全屏查看器（独立窗口方案，已完成）

- [x] **独立全屏窗口**：主窗口只发载荷，查看器跑在 `image-fullscreen` 窗口里（`commands/image_fullscreen.rs` + `features/image/FullscreenWindow.vue`），两个详情弹窗改为 `open_image_fullscreen({ items, index })`。结论见 docs/lessons.md 第 18 节、docs/开发经验.md 第 3 节。

## 下一步开发

- [ ] **新增 e2e：全屏查看界面各元素显示正常**：双击大图进入查看窗口，断言图像本体、文件名、标签、导航与索引、右上 ✕ 都在且可见；点 ✕ 退出后详情弹窗回到原样（尺寸与内容不变）。
  - 前置依赖：先完成下面「e2e 多窗口适配」——查看器窗口会让 CDP 多出一个 page/target，现有「找应用页面」的逻辑会挑错页面。

## 待办

- [ ] **e2e 多窗口适配**（暂缓）：`e2e/e2e-helpers.ts` 里「找应用页面」的逻辑要能按窗口 label / URL 区分主窗口与查看器窗口。当前 spec 没有双击进查看器的用例，暂不受影响；上面那条 e2e 用例必须先做这个适配。
- [ ] **可重构点：图像 URL 解析收敛**：现在三处各自拼 `convertFileSrc`——`FullscreenWindow.vue` 的 `resolveSrc` / `resolveMeta`（按 id 调后端）、`ImageDetailModal.vue` 的 `origSrc`（走 `imageSrcCache` 300 条 LRU）、`PromptDetailModal.vue` 的 `imgUrl`（直接用列表带回的路径）；缓存策略不同但入口重复，可考虑收敛到 `features/image/api/`，把「缓存策略」与「URL 拼装」分开。
