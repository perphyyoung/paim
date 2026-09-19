# todo

本文件仅供临时性的进度追踪，其它文件不得引用。

## 全屏查看器（独立窗口方案，已实施）

- [x] **B 独立全屏窗口**：主窗口只发载荷，查看器跑在 `image-fullscreen` 窗口里（`commands/image_fullscreen.rs` + `features/image/FullscreenWindow.vue`），两个详情弹窗改为 `open_image_fullscreen({ items, index })`。结论与取舍见 docs/lessons.md 第 18 节。
- [ ] **e2e 适配（暂缓，按用户要求先不改）**：查看器窗口会让 CDP 多一个 page/target，`e2e/e2e-helpers.ts` 里「找应用页面」的逻辑需要能按窗口 label/URL 区分。当前 spec 没有双击进查看器的用例，暂不受影响；将来要给查看器加用例时必须先改这里。
- [ ] 可选增强：从查看器窗口返回时，把查看器内的当前索引同步回详情弹窗（现状与旧实现一致：不联动，关闭后详情仍停在原图）。
