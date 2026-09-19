# todo

本文件仅供临时性的进度追踪，其它文件不得引用。

## 全屏查看器（独立窗口方案，已完成）

- [x] **独立全屏窗口**：主窗口只发载荷，查看器跑在 `image-fullscreen` 窗口里（`commands/image_fullscreen.rs` + `features/image/FullscreenWindow.vue`），两个详情弹窗改为 `open_image_fullscreen({ items, index })`。结论见 docs/lessons.md 第 18 节、docs/开发经验.md 第 3 节。
- [x] **e2e 多窗口适配**：`e2e-helpers.ts` 新增 `findPageByWindowLabel`（按窗口 label 找页面——查看器与主窗口 url 相同，只能靠 label 区分）、`openFullscreenViewer` / `closeFullscreenViewer`、`addItemTag`；Rust 侧新增测试缝 `e2e_is_window_visible`。
- [x] **e2e：全屏查看界面各元素显示正常**：`e2e/10-open-fullscreen-in-image-detail-modal.spec.ts`。

## 可重构项（已完成）

- [x] **图像 URL 解析收敛**：新增 `src/utils/assetUrl.ts` 的 `toAssetUrl`（`convertFileSrc` 的唯一出口，空串兜底），全仓 13 处调用全部改走它；原图 URL 的取值入口收进 `features/image/api/detailCache.ts`（`peekImageSrc` 同步命中 / `resolveImageSrc` 异步取），详情弹窗与独立查看器窗口共用；附前端单测。
