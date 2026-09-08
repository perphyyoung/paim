# todo.md

## 详情弹窗重复读库优化 + 可选重构（已完成，待本地验证）

### 一级：前端实体级缓存

- [x] `src/utils/entityCache.ts`：`createEntityCache` 工厂（模块级 Map + in-flight 去重）
- [x] `src/features/prompt/api/relatedImagesCache.ts`：`relatedImagesCache`（关联图像）
- [x] `PromptDetailModal.vue`：命中缓存直接渲染、不进 loading；切换后预取相邻项；失效点 = 移除关联/设为首图/导入/嵌套换图
- [x] `src/features/image/api/detailCache.ts`：`relatedPromptsCache` / `imageSrcCache` / `imageTagsCache`
- [x] `ImageDetailModal.vue`：三档走缓存；失效点 = 解除关联/新建提示词/嵌套提示词 updated/标签增删/安全评级联动

### 二级：后端 N+1

- [x] `domain/prompt_service.rs::list_related_images`：标签移出循环批量查；拆出 `list_related_images_with`
- [x] `domain/image_service.rs::list_related_prompts`：逻辑从命令层下沉；标签同样批量查
- [x] `domain/tag_manager.rs::tags_by_owner`：两域共用，按 `TagDomain` 映射表名/列名，消除 redup
- [x] `commands/image.rs::get_image_related_prompts`：瘦身为薄壳
- [x] `prompt_service.test.rs` / `image_service.test.rs`：补「多项关联+不同标签」用例
- [x] `CHANGE.md` v0.2.15 加一条

### 可选重构（已完成）

- [x] 前端两处缓存统一为 `createEntityCache` 工厂
- [x] 后端两个 `fill_*_tags` 统一为 `tag_manager::tags_by_owner`

### 收尾（交用户本地）

- [ ] `pnpm format:rs`（我这边 cargo fmt / cargo test 撞 0xC0000005，同上环境问题）
- [ ] `pnpm test`
- [ ] `sentrux check .`
- [ ] `pnpm check`
