# 设计规范

> 目的：统一界面「动作-颜色」映射与组件使用约定，消除「保存/取消」等按钮因配色相似造成的误导。
> 适用：全部 Vue 组件，Tailwind 类。深色主题专属，无亮暗分支。
> 新增界面请直接引用本文中的 class / 组件，不另创颜色、不手写标签类。

## 一、铁律

1. **蓝只用于正向确认动作**；取消/关闭/退出一律中性描边，禁用蓝色实心。
2. **红只用于破坏性动作**，且必须前置 danger 确认弹窗（`ConfirmDialog` / `InlineDialog` 的 `danger`），确认键为红实心。
3. **绿=安全、琥珀=收藏**，只表达状态，不做按钮主色。

## 二、颜色

### 设计原则

**颜色 = 动作语义**，而非装饰。决定一个控件颜色前，先回答：这是「正向确认」「退出/取消」「破坏性」「状态展示」中的哪一类？

| 语义                 | 视觉       | 典型文案                     |
| -------------------- | ---------- | ---------------------------- |
| 正向确认（Primary）  | 蓝色实心   | 保存、新建、导入、确定、提交 |
| 退出/取消（Neutral） | 中性描边   | 取消、关闭、退出、返回       |
| 破坏性（Danger）     | 红色       | 删除、移除、清空、彻底删除   |
| 状态展示             | 绿/红/琥珀 | 安全、敏感、收藏、标签 chip  |

### 动作色（按钮）

1. Primary — 蓝实心（只用于正向确认动作）

    ```html
    <button class="rounded-lg bg-blue-600 px-4 py-2 text-sm font-medium text-white transition-colors hover:bg-blue-500 disabled:cursor-not-allowed disabled:opacity-50" />
    ```

1. Neutral — 中性描边（取消/关闭/退出/返回）

   - 小尺寸图标/文字钮同理：`px-2 py-1 text-xs`，边框与底色一致走 gray 系。

    ```html
    <button class="rounded-lg border border-gray-600 px-4 py-2 text-sm text-gray-200 transition-colors hover:bg-gray-700" />
    ```

1. Danger — 红色（破坏性）

   - 确认弹窗/内嵌确认的确认键，使用红实心：

    ```html
    <button class="rounded-lg bg-red-600 px-4 py-2 text-sm font-medium text-white hover:bg-red-500 disabled:cursor-not-allowed disabled:opacity-50" />
    ```

   - 图标入口（删除/移除按钮）用红文字：

    ```html
    <button class="text-red-400 hover:text-red-300" />
    ```

1. Disabled — 禁用

   - 统一 `disabled:cursor-not-allowed disabled:opacity-50`。

### 状态与展示色（表达状态，非操作按钮）

| 状态/用途          | 色（Tailwind class）                                                    | 示例                                             |
| ------------------ | ----------------------------------------------------------------------- | ------------------------------------------------ |
| 安全               | 绿 `bg-green-500`                                                       | 安全 toggle 开启态                               |
| 敏感/不安全        | 红 `bg-red-500`                                                         | 安全 toggle 关闭态                               |
| 收藏               | 琥珀渐变 `from-amber-500 to-amber-400`                                  | 收藏按钮激活态；卡片边框 `border-amber-500`      |
| 编辑中标识         | 浅蓝 `bg-blue-900/30 text-blue-300`                                     | 详情页编辑态按钮（不用蓝实心）                   |
| 选中态（列表）     | 靛蓝 `bg-indigo-500/15`                                                 | MediaCard 选中遮罩；checkbox `accent-indigo-500` |
| 选中态（索引）     | 靛蓝 `bg-indigo-900/30 text-indigo-300`                                 | 图像详情关联提示词索引选中                       |
| 标签               | 紫色系 `bg-purple-600/25` / `bg-purple-500`                             | 见「标签」章节                                   |
| 信息浅底 chip      | 蓝 `bg-blue-900/40 text-blue-300` / 绿 `bg-green-900/40 text-green-300` | 标签管理排序序号、首位组标识                     |
| 选择项选中（弹窗） | 蓝 `border-blue-500 bg-blue-500 text-white`                             | 图片选择弹窗选中图                               |
| 拖放目标高亮       | 蓝环 `ring-2 border-blue-500 ring-blue-500/40`                          | 标签管理拖拽落点                                 |
| 拖拽跟随浮层       | 蓝 `bg-blue-600 text-white`                                             | 标签管理拖拽中的组名浮层                         |
| 错误提示           | 红 `text-red-400`、错误框 `bg-red-900/30 text-red-400`                  | 各弹窗错误信息                                   |

## 三、标签（TagChip）

> 全部标签统一使用 `TagChip` 组件，交互/形状差异由 props 控制，不手写标签类。
> 颜色语义：**未选中 = 浅紫底，选中 = 深紫底，前景恒白**；计数徽章恒蓝底白字。

### 变体（variant）

| 变体              | 视觉（Tailwind class）                                                                        | 用途                                        |
| ----------------- | --------------------------------------------------------------------------------------------- | ------------------------------------------- |
| `checked`（默认） | `bg-purple-600/25 border border-purple-400/50 text-white`，hover `hover:border-purple-300/60` | 未选中/基础色（详情页、管理页、卡片、全屏） |
| `solid`           | `bg-purple-500 text-white hover:bg-purple-400`                                                | 标签筛选**选中态**                          |

### 计数徽章（count）

左上角绝对定位，`bg-blue-600` 蓝底白字（18px 圆角），**选中/未选中都不变色**。

### 删除按钮（removable = true）

右上角 `-right-2 -top-2` 深灰圆钮（`bg-gray-800 border border-gray-600`）红色 ✕，`opacity-0 group-hover:opacity-90` hover 才显示，点击派发 `remove`。

### 尺寸（size）

| size         | class                        | 场景                                           |
| ------------ | ---------------------------- | ---------------------------------------------- |
| `md`（默认） | `px-2.5 py-0.5 text-xs`      | 筛选区、详情页、管理页                         |
| `sm`         | `px-1 text-[10px] leading-4` | 卡片行（CardTagRow）、图上左下覆盖、全屏查看器 |

### 场景映射

| 场景                        | 用法                                                               |
| --------------------------- | ------------------------------------------------------------------ |
| 标签筛选区                  | `interactive` + `count`，未选中 `checked` / 选中 `solid`           |
| 标签管理页                  | `count` + `dimOnHover`，编辑（铅笔）/删除（✕）角按钮由外层容器提供 |
| 提示词/图像详情标签         | `removable`（✕ 右上角 hover 显示），其余只读                       |
| 卡片行                      | `size="sm"`，由 CardTagRow 接管测量与「+n」汇聚                    |
| 全屏查看器 / 图像左下角覆盖 | `size="sm"` 只读                                                   |

## 四、z-index 层级

> 目的：统一全屏弹层的堆叠关系，避免「高 z 弹窗盖住低 z 遮罩」导致点不中、关不掉（如右键菜单）。
> 约定：**遮罩与本体成对出现，遮罩略低于本体；新弹层只能占用「空档」或比当前最高层更高，不得插队同层。**

### 全局弹层（fixed / Teleport to body，从低到高）

| z   | 元素                  | 来源                                                                                                                         | 说明                                    |
| --- | --------------------- | ---------------------------------------------------------------------------------------------------------------------------- | --------------------------------------- |
| 50  | 业务弹窗本体          | ImageDetailModal / PromptDetailModal / TrashOverlay / TagManagerModal / ImagePickerModal / NewPromptModal / ImageUploadModal | 普通弹窗/整页覆盖，最低一层全屏层       |
| 60  | 右键菜单遮罩          | ContextMenu                                                                                                                  | 须盖过全部 z-50 弹窗，点击即关          |
| 60  | 设置弹窗              | App.vue                                                                                                                      | 与右键遮罩同层（场景互斥）              |
| 60  | 内嵌子对话框          | InlineDialog（TagManagerModal 内嵌 dlg / ImageDetailModal 新建提示词）                                                       | 弹窗内的子对话框，低于右键菜单本体      |
| 70  | 右键菜单本体          | ContextMenu                                                                                                                  | 高于自身遮罩 1 挡                       |
| 70  | 全屏查看器            | ImageFullscreenViewer                                                                                                        | 详情页之上的查看层                      |
| 90  | 标签拖拽跟随浮层      | TagManagerModal                                                                                                              | 纯展示，pointer-events-none             |
| 100 | 批量操作工具条        | BatchActionBar                                                                                                               | 悬浮工具条，不挡操作                    |
| 110 | 确认弹窗              | ConfirmDialog / BatchActionBar 内确认                                                                                        | 最高确认层                              |
| 120 | 备份导入 / 缩略图重建 | PmBackupImportModal / ThumbnailRebuildModal                                                                                  | 顶层模态                                |
| 130 | Toast                 | ToastHost                                                                                                                    | 永驻最高层，pointer-events-none，仅展示 |

### 组件内局部层级（非全屏，仅作用于自身 stacking context）

| z     | 元素                                | 来源                  |
| ----- | ----------------------------------- | --------------------- |
| 1     | 卡片选中遮罩（pointer-events-none） | MediaCard             |
| 3     | 卡片顶部标签行                      | MediaCard             |
| 1 / 2 | 计数徽章 / 删除钮                   | TagChip               |
| 2     | 行内置顶、删除角钮                  | TagManagerModal 行    |
| 10    | 底部图例条 / 提交条                 | 详情弹窗、全屏查看器  |
| 20    | 全屏右上关闭钮                      | ImageFullscreenViewer |

### 关联关系与注意

- **同一层内场景互斥**时可共用 z 值（如 z-60 设置弹窗与右键遮罩、z-50 各业务弹窗），互不叠加出现，DOM 顺序即决定谁在上。
