# 设计方案

> 目的：统一「动作 → 颜色」映射，消除「保存/取消」等按钮因配色相似造成的误导。
> 适用：全部 Vue 组件，Tailwind 类。深浅色模式均需覆盖（`dark:` 前缀）。

## 一、总原则

**颜色 = 动作语义**，而非装饰。决定一个控件颜色前，先回答：这是「正向确认」「退出/取消」「破坏性」「状态展示」中的哪一类？

| 语义 | 视觉 | 典型文案 |
|---|---|---|
| 正向确认（Primary） | 蓝色实心 | 保存、新建、导入、确定、提交 |
| 退出/取消（Neutral） | 中性描边 | 取消、关闭、退出、返回 |
| 破坏性（Danger） | 红色 | 删除、移除、清空、彻底删除 |
| 状态展示 | 绿/红/琥珀 | 安全、敏感、收藏、标签 chip |

## 二、动作色阶

### 1. Primary — 蓝实心（只用于正向确认动作）
```html
<button class="rounded-lg bg-blue-600 px-4 py-2 text-sm font-medium text-white transition-colors hover:bg-blue-500 disabled:cursor-not-allowed disabled:opacity-50" />
```
- 深色模式无需改（蓝色通用）。
- **铁律：取消/关闭/退出绝不用蓝实心。**

### 2. Neutral — 中性描边（取消/关闭/退出/返回）
```html
<button class="rounded-lg border border-gray-300 px-4 py-2 text-sm text-gray-700 transition-colors hover:bg-gray-100 dark:border-gray-600 dark:text-gray-200 dark:hover:bg-gray-700" />
```
- 小尺寸图标/文字钮同理：`px-2 py-1 text-xs`，边框与底色一致走 gray 系。

### 3. Danger — 红色（破坏性）
确认弹窗的确认键，使用红实心：
```html
<button class="rounded-lg bg-red-600 px-4 py-2 text-sm font-medium text-white hover:bg-red-700" />
```
图标入口（删除/移除按钮）用红文字：
```html
<button class="text-red-600 hover:text-red-700 dark:text-red-400 dark:hover:text-red-300" />
```
- **铁律：破坏性动作必前置确认弹窗（ConfirmDialog danger）**，禁止直接红键即删。

### 4. Disabled — 禁用
统一 `disabled:cursor-not-allowed disabled:opacity-50`。

## 三、状态色（表达状态，非操作按钮）

| 状态 | 色 | 示例 |
|---|---|---|
| 安全 | 绿 `bg-green-500` | 安全 toggle 开启态 |
| 敏感/不安全 | 红 `bg-red-500` | 安全 toggle 关闭态 |
| 收藏 | 琥珀渐变 `from-amber-500 to-amber-400` | 收藏按钮激活态 |
| 编辑中标识 | 蓝 | 编辑态文字加粗 `font-medium dark:text-gray-200`，不用蓝实心 |

## 四、展示性元素（允许浅蓝，不算动作色）

- **标签 chip（展示/筛选）**：`bg-blue-100 text-blue-700 dark:bg-blue-900/40 dark:text-blue-300` —— 属信息展示，非「确认动作」，允许蓝。仅**实心蓝**保留给动作。

## 五、3 条铁律

1. **蓝只用于正向确认动作**；取消/关闭/退出一律中性描边，禁用蓝色实心。
2. **红只用于破坏性动作**，且必须前置确认弹窗。
3. **绿=安全、琥珀=收藏**，只表达状态，不做按钮主色。

## 六、现状核查清单

- [x] 提示词详情右上角「编辑/取消」：取消态已由蓝实心改回中性描边（与底部「取消」一致）。
- [ ] 复查各页「新建/导入/保存/确认」是否统一 `bg-blue-600`。
- [ ] 复查各页「取消/关闭」是否统一中性描边、无蓝实心。
- [ ] 复查删除/移除是否均走 `ConfirmDialog` 且确认键为红。
- [ ] 复查安全 toggle、收藏按钮状态色是否按上表。

> 新增界面请直接引用上述 class，不另创颜色。
