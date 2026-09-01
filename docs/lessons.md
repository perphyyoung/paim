# Lessons — 排查记录

## 1. 冷启动即弹出「详情弹窗」，及由此引入的「详情数据不加载」问题

### 现象

应用冷启动后，未做任何操作，自动弹出（提示词/图像）详情界面；修好后，打开详情时关联图像、标签等子数据区域又为空、不加载。

### 排查过程

1. 在前端 `main.ts` 的 `router.afterEach`、`App.vue` 的 `onMounted` 追加启动诊断日志。
2. 启动后扫描 `body` 上的 `.fixed.inset-0` 全屏遮罩元素，日志显示启动时确实有弹窗残留。
3. 分析详情组件结构定位「弹窗残留」根因（见下），修复后冷启动不再弹窗；但随即发现详情子数据不加载，进一步定位是修复方案引入的副作用。

### 根因

- 弹窗残留：`PromptDetailModal.vue` / `ImageDetailModal.vue` 以 `<Teleport>` 作为模板根（多根组件），又嵌套多个同样 `Teleport` 到 `body` 的兄弟组件（确认弹窗、图像详情）。此版本 Vue 对这种「多根 + 嵌套 Teleport」的卸载不可靠，卸载时 `Teleport` 节点未随父级移除，残留遮罩表现为「自动弹窗」。
- 数据不加载（修复的副作用）：为修弹窗残留，给详情组件加父级 `v-if="detailOpen"`，组件改为「打开时才挂载」。组件内部原先靠 `watch(() => [props.open, props.initialIndex])` 在 `open` 从 `false→true` 跳变时触发初始化（设 `index`、`loadOrig`/`loadTags`/`loadRelatedImages` 等）。父级 `v-if` 后挂载时 `open` 已是 `true`，该 watch 不再触发，所有子数据加载函数从未调用。

> 说明：Vue 中 `<Teleport>` 只能有一个根节点，`<Teleport>` 本身作为模板根时组件即多根；且 `Teleport` 不支持 `transition`，常造成卸载/动画时序问题。

### 修复

1. 弹窗残留：在父级页面（`PromptPage.vue` / `ImagePage.vue`）给详情组件加父级 `v-if`，强制整体卸载，让含内部全部 `Teleport` 子树的组件在关闭时被彻底销毁。
2. 数据不加载：给「打开即初始化」的 watch 加 `{ immediate: true }`，挂载即执行首次加载：

```ts
watch(
  () => [props.open, props.initialIndex] as const,
  ([open, initIdx]) => {
    if (open) {
      index.value = initIdx;
      syncFields();
      loadOrig();
      loadTags();
      loadRelatedPrompts();
    }
  },
  { immediate: true } // 组件挂载即初次加载（父级 v-if 强制卸载后依赖此初始化）
);
```

### 后续参考 / 通用约束

- 需要「整弹窗关闭即卸载」的弹窗组件，慎用「嵌套 `Teleport`-到-body」结构；必须用时，用父级 `v-if` 兜底显式销毁整棵子树。
- 任何被父级 `v-if` 强制卸载、且依赖 `open` 从 `false→true` 初始化数据的弹窗组件，初始加载 watch 务必加 `immediate`（或改用 `onMounted` 触发）。
- 排查「组件打开了但子数据为空」时，先确认入口 watch 是否有 `immediate`，不要误以为是后端接口不返回数据。
