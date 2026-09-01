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

## 2. Windows「打开本地保存位置」定位到桌面/默认位置

### 现象

图像卡片右键「打开本地保存位置」，Explorer 总是打开到桌面/默认位置，而不是选中目标文件。设置里的「打开数据目录」（直接 `explorer <dir>`）一直正常，唯独带 `/select,` 定位文件时失败。

### 排查过程

1. 拼接 `explorer /select,<full>` 字符串单参数调用 → 失败。
2. 追加调试日志打印解析出的完整路径与存在性（`resolved`/`full.exists()`），路径本身正确、文件存在。
3. 与参考项目 lap（`reveal_path`）对齐后恢复正常，差异集中在两点（见下）。

### 根因/正确做法

Explorer 的 `/select,` 需要单独作为一个参数传入，与路径分开（`arg("/select,").arg(path)`）；且路径中若有正斜杠（数据库 `relative_path` 存 `/`，`Path::join` 会保留），混用分隔符会让 Explorer 回退到默认位置。需将路径统一为反斜杠：`path.replace('/', "\\")`。

```rust
let norm = full.to_string_lossy().replace('/', "\\");
std::process::Command::new("explorer")
    .arg("/select,")
    .arg(norm)
    .spawn()
    .map_err(|e| AppError::Message(format!("打开保存位置失败: {e}")))?;
```

注意：`explorer` 是独立进程（不等待返回值），用 `spawn()` 即可；返回码几乎总是 0，不能靠它判断是否定位成功。

### 后续参考 / 通用约束

- Windows 用 `explorer /select,<file>` 定位文件时：参数必须拆分传（`/select,` 与路径分开），路径必须全反斜杠。
- 排查同类问题时先与参考项目（lap 等）比对参数调用形式，不要先在业务代码里加条件/回退逻辑。
