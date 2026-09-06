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

## 3. tauri-specta 集成：BigInt 绕过方式错误与前端迁移

### 现象

集成 tauri-specta 自动生成 TS 绑定时，导出报「BigInt 类型禁止导出」错误；为绕过限制把一批业务类型从 `usize`/`i64` 改成 `i32`，引入 16 个编译错误并造成大范围返工；后续把 i32 还原为原始类型时又出现残留；前端迁移阶段还有若干类型不兼容报错。

### 根因与教训

1. **猜 API 而非查文档**：逐字段加 `#[specta(type = Number)]` 是字段级覆盖手段，但官方对「BigInt 类型导出」的推荐方式是 Builder 级配置 `dangerously_cast_bigints_to_number()`，一行全局生效、业务类型零侵入。为绕过导出限制去修改运行时类型（usize→i32）是本末倒置——类型应服务运行时语义，不应为绑定导出让路。
2. **rc 版本文档不可用时读源码**：docs.rs 对 rc 版本经常构建失败（rc.25 即失败）。可靠途径是让 cargo 下载宏 crate（启用对应 feature 后），直接读本地 registry 里的宏源码——`Event` derive 的命名规则（默认类型名 kebab-case）与 `#[tauri_specta(event_name = "...")]` 属性就是读源码确认的。
3. **确认默认行为再选型**：`ErrorHandlingMode` 默认是 `Result`（判别联合 `{ status, data/error }`），与项目既有 try/catch + toast 模式不匹配；两种模式都官方支持，选 `Throw`（Promise reject）迁移量最小。决策前先查 enum 的 `#[default]`。
4. **replace_all 批量替换有遗漏风险**：用 `id: i32,`（带逗号）模式还原时漏掉了 `id: i32)`（右括号）形式的 9 处。批量替换后必须 `rg` 全局扫描确认清零，不能只看编译通过（此处 i32 传 i32 恰好能编译）。
5. **编辑事故残留会伪装成新问题**：还原过程中发现此前编辑事故写入的字面 `` `n `` 字符残留在两个 `restore_all` 函数中（还丢了 `?` 与右括号），排查时先排除文件本身被污染的可能。
6. **前端迁移的机械差异**：bindings 命令是**位置参数 + camelCase**（内部组包 snake_case 对象）；模板字符串拼命令名（`` `add_${domain}_tag` ``）必须改为 bindings 函数分派；本地重复类型定义应删除并从 bindings 导入（api 层用 re-export 保持下游别名兼容）；后端 `Option<T>` 字段导出为 `T | null` 且可能带 `?` 可选标记，组件 props 逐层传播时要同步兼容。

### 后续参考 / 通用约束

- 遇到 specta 导出限制，先查 Builder 配置（`dangerously_cast_bigints_to_number` / `error_handling` 等），禁止为绑定导出修改业务类型。
- 批量类型替换后必须全局 `rg` 扫描验证清零；「编译通过」不等于「替换完整」。
- 集成新库前用三处交叉确认 API：docs.rs 版本页 → Builder 源码 → 宏 crate 源码；不猜 API。
- 修改命令签名后重新跑 `pnpm dev` 重新生成 `src/bindings.ts`，`vue-tsc` 会立即报出所有失配调用点——这正是类型安全绑定的核心价值。

## 4. e2e 失败排查：完整控制台输出才有报错行数，error-context.md 只有页面快照

### 现象

e2e 用例失败时，只读 `test-results/*/error-context.md` 定位——里面只有失败时的页面 ARIA 快照（且可能只有一两行），
**没有报错行号与调用日志**，导致反复猜测失败在哪一步。

### 根因与正确做法

`error-context.md` 是给「页面状态」参考的；**报错行数、调用日志（如「元素被某遮罩拦截 pointer events」的具体细节）
都在 playwright 的完整控制台输出里**。排查必须看运行命令的完整输出文件（如 `pnpm e2e > x.log 2>&1` 后读 x.log），
不要只看 error-context.md，也不要用 grep/head 截断输出。

### 后续参考 / 通用约束

- e2e 失败排查顺序：先读完整控制台输出（定位报错行 + call log），再按需看 error-context.md 的页面快照。
- 输出里有「`<div ...> intercepts pointer events`」类信息 = 点击被别的元素遮挡；有「waiting for ...」= 元素未出现。

## 5. e2e 同 worker 用例间残留 UI 状态：上一用例的弹窗挡住下一用例

### 现象

同一个 spec 文件里前一个用例结束后，后一个用例一开头点击侧边栏导航就超时——完整控制台输出显示
`<div class="fixed inset-0 z-50 ...">intercepts pointer events`：上一用例结束后**详情弹窗仍开着**（应用状态在
worker 内跨用例保留），其全屏遮罩挡住了后续所有点击。

### 根因与正确做法

worker 级 fixture 的应用实例跨用例共享，UI 状态（打开的弹窗）不会自动复位。复位统一放在 `helpers.ts` 的
test 级 `page` fixture 里：**每个用例开始前 reload 一次**（数据都在库里，重载无副作用），用例内不要自行 reload；
**worker 首个用例跳过 reload**——全新实例无残留，且 reload 会打断初始加载中的 IPC 请求
（ERR_ABORTED + 「Couldn't find callback id」回调失联），可能让后续 invoke 挂起。

### 后续参考 / 通用约束

- 同文件的多个用例共享同一应用实例：写用例时默认「上一用例可能残留打开的弹窗/状态」，但复位由 `page` fixture 统一处理，用例内不要重复 reload。
- 首个用例不要 reload：全新实例无残留，强行 reload 反而撞上初始加载导致回调失联。
- 排查时若 call log 出现 `intercepts pointer events`，先找是谁的遮罩（常见：上一用例没关的弹窗、未消失的 toast）。
