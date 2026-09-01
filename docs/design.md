# 设计方案

> 目的：统一「动作 → 颜色」映射，消除「保存/取消」等按钮因配色相似造成的误导。
> 适用：全部 Vue 组件，Tailwind 类。深浅色模式均需覆盖（`dark:` 前缀）。

## 一、总原则

**颜色 = 动作语义**，而非装饰。决定一个控件颜色前，先回答：这是「正向确认」「退出/取消」「破坏性」「状态展示」中的哪一类？

| 语义                 | 视觉       | 典型文案                     |
| -------------------- | ---------- | ---------------------------- |
| 正向确认（Primary）  | 蓝色实心   | 保存、新建、导入、确定、提交 |
| 退出/取消（Neutral） | 中性描边   | 取消、关闭、退出、返回       |
| 破坏性（Danger）     | 红色       | 删除、移除、清空、彻底删除   |
| 状态展示             | 绿/红/琥珀 | 安全、敏感、收藏、标签 chip  |

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

| 状态        | 色                                     | 示例                                                        |
| ----------- | -------------------------------------- | ----------------------------------------------------------- |
| 安全        | 绿 `bg-green-500`                      | 安全 toggle 开启态                                          |
| 敏感/不安全 | 红 `bg-red-500`                        | 安全 toggle 关闭态                                          |
| 收藏        | 琥珀渐变 `from-amber-500 to-amber-400` | 收藏按钮激活态                                              |
| 编辑中标识  | 蓝                                     | 编辑态文字加粗 `font-medium dark:text-gray-200`，不用蓝实心 |

## 四、展示性元素（允许浅蓝，不算动作色）

### 标签 —— 统一使用 `TagChip` 组件（新增/手写标签一律引用它）

> 深色主题专属，不再写亮色/暗色分支，也不手写标签类。交互/形状差异由 props 控制。
> 颜色语义：**未选中 = 浅紫底，选中 = 深紫底，前景恒白**；计数徽章恒蓝底白字。

**变体（variant）**：

| 变体              | 视觉（Tailwind class）                                                                        | 用途                                        |
| ----------------- | --------------------------------------------------------------------------------------------- | ------------------------------------------- |
| `checked`（默认） | `bg-purple-600/25 border border-purple-400/50 text-white`，hover `hover:border-purple-300/60` | 未选中/基础色（详情页、管理页、卡片、全屏） |
| `solid`           | `bg-purple-500 text-white hover:bg-purple-400`                                                | 标签筛选**选中态**                          |

**计数徽章（count）**：左上角绝对定位，`bg-blue-600` 蓝底白字（18px 圆角），**选中/未选中都不变色**。

**删除按钮（removable = true，与管理页一致）**：右上角 `-right-2 -top-2` 深灰圆钮（`bg-gray-800 border border-gray-600`）红色 ✕，`opacity-0 group-hover:opacity-90` **hover 才显示**，点击派发 `remove`。

**尺寸（size）**：

| size         | class                        | 场景                                           |
| ------------ | ---------------------------- | ---------------------------------------------- |
| `md`（默认） | `px-2.5 py-0.5 text-xs`      | 筛选区、详情页、管理页                         |
| `sm`         | `px-1 text-[10px] leading-4` | 卡片行（CardTagRow）、图上左下覆盖、全屏查看器 |

**场景映射**：

| 场景                        | 用法                                                               |
| --------------------------- | ------------------------------------------------------------------ |
| 标签筛选区                  | `interactive` + `count`，未选中 `checked` / 选中 `solid`           |
| 标签管理页                  | `count` + `dimOnHover`，编辑（铅笔）/删除（✕）角按钮由外层容器提供 |
| 提示词/图像详情标签         | `removable`（✕ 右上角 hover 显示），其余只读                       |
| 卡片行                      | `size="sm"`，由 CardTagRow 接管测量与「+n」汇聚                    |
| 全屏查看器 / 图像左下角覆盖 | `size="sm"` 只读                                                   |

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

## 七、z-index 层级

> 目的：统一全屏弹层的堆叠关系，避免「高 z 弹窗盖住低 z 遮罩」导致点不中、关不掉（如右键菜单）。
> 约定：**遮罩与本体成对出现，遮罩略低于本体；新弹层只能占用「空档」或比当前最高层更高，不得插队同层。**

### 1. 全局弹层（fixed / Teleport to body，从低到高）

| z       | 元素                          | 来源                                                                                                                         | 说明                              |
| ------- | ----------------------------- | ---------------------------------------------------------------------------------------------------------------------------- | --------------------------------- |
| 50      | 业务弹窗本体                  | ImageDetailModal / PromptDetailModal / TrashOverlay / TagManagerModal / ImagePickerModal / NewPromptModal / ImageUploadModal | 普通弹窗/整页覆盖，最低一层全屏层 |
| 60      | 右键菜单遮罩                  | ContextMenu                                                                                                                  | 须盖过全部 z-50 弹窗，点击即关    |
| 60      | 设置弹窗                      | App.vue                                                                                                                      | 与右键遮罩同层（场景互斥）        |
| 60      | 标签管理内嵌输入/确认         | TagManagerModal 内嵌 dlg                                                                                                     | 弹窗内的子对话框                  |
| 70      | 右键菜单本体                  | ContextMenu                                                                                                                  | 高于自身遮罩 1 挡                 |
| 70      | 全屏查看器                    | ImageFullscreenViewer                                                                                                        | 详情页之上的查看层                |
| 70      | 图像详情内嵌「新建提示词」    | ImageDetailModal 内嵌 createPrompt                                                                                           | 弹窗内的子对话框                  |
| 80 / 81 | 标签管理右键菜单（遮罩/本体） | TagManagerModal 手写菜单                                                                                                     | 未复用 ContextMenu                |
| 90      | 标签拖拽跟随浮层              | TagManagerModal                                                                                                              | 纯展示，pointer-events-none       |
| 100     | Toast                         | ToastHost                                                                                                                    | pointer-events-none，仅展示       |
| 100     | 批量操作工具条                | BatchActionBar                                                                                                               | 悬浮工具条，不挡操作              |
| 110     | 确认弹窗                      | ConfirmDialog / BatchActionBar 内确认                                                                                        | 最高确认层                        |
| 120     | 备份导入 / 缩略图重建         | PmBackupImportModal / ThumbnailRebuildModal                                                                                  | 顶层模态                          |

### 2. 组件内局部层级（非全屏，仅作用于自身 stacking context）

| z     | 元素                                | 来源                  |
| ----- | ----------------------------------- | --------------------- |
| 1     | 卡片选中遮罩（pointer-events-none） | MediaCard             |
| 3     | 卡片顶部标签行                      | MediaCard             |
| 1 / 2 | 计数徽章 / 删除钮                   | TagChip               |
| 2     | 行内置顶、删除角钮                  | TagManagerModal 行    |
| 10    | 底部图例条 / 提交条                 | 详情弹窗、全屏查看器  |
| 20    | 全屏右上关闭钮                      | ImageFullscreenViewer |

### 3. 关联关系与注意

- **同一层内场景互斥**时可共用 z 值（如 z-60 设置弹窗与右键遮罩、z-50 各业务弹窗），互不叠加出现，DOM 顺序即决定谁在上。
- **同类语义取值已不一致**，后续新增请对齐：
  - 右键菜单：公共 ContextMenu 为 60/70，TagManagerModal 手写为 80/81 → 建议统一为公共组件。
  - 弹窗内嵌子对话框：TagManagerModal 为 60，ImageDetailModal 内嵌新建提示词为 70 → 建议统一为 60（低于右键菜单本体 70）。
- Toast 在 z-100，确认弹窗 z-110 时 Toast 会被盖住；Toast 仅展示且 pointer-events-none，可接受，无须改动。
