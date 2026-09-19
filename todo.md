# todo

本文件仅供临时性的进度追踪，其它文件不得引用。

## 全屏查看器（独立窗口方案，已完成）

- [x] **独立全屏窗口**：主窗口只发载荷，查看器跑在 `image-fullscreen` 窗口里（`commands/image_fullscreen.rs` + `features/image/FullscreenWindow.vue`），两个详情弹窗改为 `open_image_fullscreen({ items, index })`。结论见 docs/lessons.md 第 18 节、docs/开发经验.md 第 3 节。
- [x] **e2e 多窗口适配**：`e2e-helpers.ts` 新增 `findPageByWindowLabel`（按窗口 label 找页面——查看器与主窗口 url 相同，只能靠 label 区分）、`openFullscreenViewer` / `closeFullscreenViewer`；Rust 侧新增测试缝 `e2e_is_window_visible`。
- [x] **e2e：全屏查看界面各元素显示正常**：`e2e/10-open-fullscreen-in-image-detail-modal.spec.ts`（图像 / 文件名 / 标签 / 导航索引 / 关闭钮齐全 + 关闭后详情弹窗不受影响）。

## 待办

- [ ] **可重构点：图像 URL 解析收敛**：现在三处各自拼 `convertFileSrc`——`FullscreenWindow.vue` 的 `resolveSrc` / `resolveMeta`（按 id 调后端）、`ImageDetailModal.vue` 的 `origSrc`（走 `imageSrcCache` 300 条 LRU）、`PromptDetailModal.vue` 的 `imgUrl`（直接用列表带回的路径）；缓存策略不同但入口重复，可考虑收敛到 `features/image/api/`，把「缓存策略」与「URL 拼装」分开。
