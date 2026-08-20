# paim 开发待办

## 提示词（prompt）

- [ ] **prompt ↔ tag 关联**
  - 创建/编辑提示词时可打标签
  - 按标签筛选提示词列表
- [ ] 提示词编辑（目前只有标题更新，content 不可改）
- [ ] 提示词搜索/全文检索

## 图像（image）

- [ ] 图像导入（选文件/拖拽到应用内）
- [ ] 读取图片元数据（尺寸、格式）
- [ ] image ↔ prompt 关联（用哪个提示词生成）
- [ ] image ↔ tag 关联 + 按标签筛选
- [ ] 图片在应用内预览/缩放
- [ ] 图像列表分页/懒加载（图片量大时）

## 通用 / 架构

- [ ] **标签去除关联引用**：删除标签时，确认 prompt_tags / image_tags 的级联清理行为
- [ ] commands 返回值统一错误类型（目前 `String`，建议 `Result<T, AppError>`）
- [ ] 数据库迁移版本化（当前用 `execute_batch` 幂等建表，需演进为带版本号的 migration）
- [ ] Rust 侧单元测试（service 层）

## 已完成的里程碑

- [x] Tauri2 + Vue3 + TS + Tailwind + rusqlite 项目骨架
