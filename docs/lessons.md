# Lessons — 排查记录

## 速查表

| 节 | 主题 | 一句话教训 |
|---|---|---|
| 1 | 冷启动自动弹详情弹窗 / 弹窗子数据为空 | 嵌套 Teleport 的多根组件卸载不可靠，需父级 `v-if` 兜底销毁；被 `v-if` 强制卸载的弹窗，初始化 watch 必须加 `immediate`。 |
| 2 | 「打开本地保存位置」落到桌面 | `explorer /select,` 必须单独成一个参数且路径统一反斜杠；它不等待返回，返回码不能判断成败。 |
| 3 | tauri-specta BigInt 导出报错 | 先查 Builder 级配置，禁止为绑定导出改业务类型；不猜 API（docs.rs 不可用时读本地 registry 宏源码）；批量替换后必须全局扫描。 |
| 4 | e2e 失败定位不到报错行 | 报错行与 call log 都在完整控制台输出里，`error-context.md` 只有页面快照。 |
| 5 | e2e 用例间点击被遮罩拦截 | worker 共享实例跨用例保留 UI 状态（弹窗没关）；复位统一放 `page` fixture（用例前 reload），worker 首用例跳过。 |
| 6 | pm 备份导入 dev 失败、release 正常 | vite chokidar 持有数据目录句柄挡住「整目录改名」；watch.ignored 改白名单，占用进程用 LockHunter/资源监视器查。 |
| 7 | e2e 上传弹窗偶发不关 | 异步测试缝（mock invoke）返回后必须等 UI 证据（预览出现文件名）再走下一步，全量跑负载高才复现。 |
| 8 | e2e 全量偶发「页面消失」 | 高负载下 webview 被销毁重建（无 crash 事件），旧 Page 引用报废；「全量偶发一红、单跑必绿」判环境偶发，不改代码。 |
| 9 | 格式化重排出无关 diff / vitest 解析失败 | 前端与 e2e 格式化只用 `pnpm format:ui`（oxfmt）；独立测试配置的隐含假设（如 `@` 别名）要随代码演进同步复核。 |
| 10 | 数据到了却一张卡片不渲染 | `shallowRef` 持有传给子组件的集合必须整体换新数组；原地改 + `triggerRef` 不变 prop 引用，子组件不重算。 |
| 11 | 万级压测切图像页「未响应」（SQL 根因） | 相关子查询即使索引齐全也可能计划退化（151s）；计数用派生表聚合、筛选用 IN/NOT IN，两域写法必须对称。 |
| 12 | 万级卡死的架构根因 | Tauri 2 同步命令在宿主主线程执行，慢 SQL 会冻结窗口；耗时命令一律 async + spawn_blocking 后台化。 |
| 13 | 压测诊断时 paim.log 0 字节 / 修复未生效 | `tauri build --debug` 前端日志全 no-op，诊断必须 `pnpm dev`；是否生效靠日志耗时对账；分步日志「开始无完成即嫌疑」。 |
| 14 | 特殊标签不显示 + 统计一直转 | 同一逻辑两份实现是「修一份漏一份」的温床；未修的慢命令占单连接锁，把排队的其他命令一起拖死。 |
| 15 | Arc 化后编译错误分两轮才清完 | 重构后用跨行扫描 + 编译器收尾；同文件编辑必须串行；投影类改造要过「命令签名/事件载荷/局部 interface/排序索引」四检查面。 |

---

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

worker 级 fixture 的应用实例跨用例共享，UI 状态（打开的弹窗）不会自动复位。复位统一放在 `e2e-helpers.ts` 的
test 级 `page` fixture 里：**每个用例开始前 reload 一次**（数据都在库里，重载无副作用），用例内不要自行 reload；
**worker 首个用例跳过 reload**——全新实例无残留，且 reload 会打断初始加载中的 IPC 请求
（ERR_ABORTED + 「Couldn't find callback id」回调失联），可能让后续 invoke 挂起。

### 后续参考 / 通用约束

- 同文件的多个用例共享同一应用实例：写用例时默认「上一用例可能残留打开的弹窗/状态」，但复位由 `page` fixture 统一处理，用例内不要重复 reload。
- 首个用例不要 reload：全新实例无残留，强行 reload 反而撞上初始加载导致回调失联。
- 排查时若 call log 出现 `intercepts pointer events`，先找是谁的遮罩（常见：上一用例没关的弹窗、未消失的 toast）。

## 6. pm 备份导入 dev 失败 release 正常：vite chokidar 监听句柄挡住数据目录改名

### 现象

`pnpm dev` 下导入 pm 备份必失败，报「备份原数据目录失败……拒绝访问 (os error 5)」；
`pnpm release` 后导入正常，且 release 在**有数据、缩略图显示中**时连续导入也正常。
paim.log 显示失败点固定：让位改名 `paim-data → paim-data_<时间戳>` 被拒
（`pm_backup_service.rs` 的 `std::fs::rename`）。关闭 VS Code/ZCode 后复现依旧。

### 排查过程

1. 对比 dev/release 导入代码路径：**零分叉**（整个导入路径无 `debug_assertions` 分支）。
2. 对比解压目录：`create_temp_dir` 用的是系统临时目录（`std::env::temp_dir()`），
   两个构建完全一致——且**失败发生在解压之前**（改名先于 `create_temp_dir`），解压目录排除。
3. 排除孤儿进程（tasklist 无 paim.exe）；排除 webview 显示句柄假设（release 有数据时二次导入正常）。
4. **LockHunter 查看 `paim-data` 的锁定进程**：`node.exe` 持有 paim-data、images、thumbnails
   等大量目录句柄——即 `pnpm dev` 拉起的 vite dev server。

### 根因

vite 的 chokidar 默认**递归监听项目根**（仅默认排除 .git/node_modules），数据目录 paim-data
在监听范围内。`pnpm dev` 运行期间 node.exe 一直持有 paim-data 内目录句柄，而导入的
「整目录改名让位」要求无任何进程持有目录树内句柄 → rename 被 os error 5 拒绝。
release 没有 vite 进程所以正常；「关编辑器无效」是因为监听者是 pnpm dev 的子进程，不是编辑器。

### 修复

`vite.config.ts` 的 `server.watch.ignored` 从黑名单改为**白名单函数**：仅监听前端运行所需
（index.html + src/ + public/），其余一律忽略。注意 package.json 随之不再被监听，
改版本号后需手动重启 vite。

### 后续参考 / 通用约束

- 「整目录改名」类操作（如数据目录让位）要求无任何进程持有目录树内句柄；失败时先查占用者，
  LockHunter / 资源监视器「关联的句柄」可直接列出进程名，不要靠猜。
- dev 环境此类占用最先怀疑**开发工具链自身**：vite chokidar、tauri CLI、编辑器 watcher——
  它们以子进程形式随 dev 常驻，关掉编辑器不等于解除占用。
- vite 项目里的数据/产物目录要么移出项目根，要么在 watch.ignored 中排除；白名单（ignored
  传函数）比黑名单更省心，新顶层目录自动豁免。
- pm 备份导入的解压目录在系统临时目录（`create_temp_dir`），与 `db::temp_dir` 无关；
  db.rs 曾有「备份导入解压」的误导注释（已更正），排查前先核对实现而非注释。

## 7. e2e mock 竞态：异步测试缝返回后必须等 UI 证据

### 现象

`pnpm e2e` 全量跑时 05-tag-autocomplete 间歇失败：上传弹窗点「确定」后不关（textarea 一直
visible 到用例超时）；单跑失败文件/单用例（`--grep`）始终通过。

### 排查过程

1. 给 `e2e-helpers.ts` 的共享 helper 加分步 `[step]` 日志（弹窗打开/选中 mock 图/填提示词/点确定/弹窗关闭），失败时把当前 toast 文本与页面可见文本快照 dump 进 paim.log（`[diag]` 行）。
2. 对比 01 用例的写法定位：01 点「选择图像」后**等待文件名出现在预览列表**再提交，共享 helper 缺这一步。

### 根因

`select_images` 测试缝是异步 invoke，helper 点「选择图像」后不等返回就点「确定」→ `files` 仍为空 →
走「请先选择图像」分支 → 弹窗不关。整文件跑时应用负载高、invoke 更慢，所以只在全量跑复现。

### 修复

`uploadImageWithPrompt` 补「预览列表出现 `e2e-upload.png`」等待断言（对齐 01 用例写法），消除竞态；
分步 `[step]` + `[diag]` 现场 dump 长期保留（噪声靠 `PAIM_E2E_LOG_LEVEL` 控制）。

### 后续参考 / 通用约束

- e2e 共享 helper 依赖异步测试缝时，等 UI 证据再走下一步，不要假设 invoke 已返回。
- 「全量才失败、单跑必过」先怀疑竞态：负载放大了异步间隔，让「碰巧先返回」的假设失效。

## 8. e2e 全量偶发「页面消失」：webview 被销毁重建，不是 renderer crash

### 现象

全量跑时另一类偶发失败：上传成功但打不开图像详情——`[diag]` 显示断言在 **0.8s 即失败**
（timeout 设了 5s 根本没等）、`page.evaluate` 读 body 失败——页面上下文已不存在。
单 worker 从未复现。

### 排查过程

先后怀疑 renderer crash 并加了 `page.on("crash")` 自愈监听；再次复现时**无 crash 事件**，
crash 假设被推翻。

### 根因

断言远小于 timeout 即失败 + evaluate 失败 = 页面上下文没了；无 crash 事件说明不是 renderer crash
（crashed 状态会派发），而是 **wry/WebView2 对渲染进程失败的处理——webview 被销毁重建**，
旧 CDP target 直接消失，旧 Page 引用报废。多 worker（4 个 WebView2 并行）高负载下偶发。
reload 旧引用救不了；根治需动态 page 引用（Proxy 转发 + 从 `browser.contexts()` 重找新页面），
本次决策不做，接受「全量偶发一红、单跑必绿」。

### 修复

crash 监听保留作保险（对真 crashed 且 target 还在的场景有效），已知对销毁重建型无效；
排查决策沉淀到 `docs/e2e测试.md`「失败排查」：全量失败先单跑失败文件，单跑通过 = 并行环境偶发，
不算回归、不追查。

### 后续参考 / 通用约束

- 断言立即失败（远小于 timeout）+ evaluate 报错 → 先查页面是否还在，不要先怀疑元素/数据。
- 无 crash 事件 + 页面立即 closed → 往 webview 被销毁重建方向查；真 crash 会派发 `page.on("crash")`。
- 「全量失败、单跑通过」先单跑再下结论，判为环境偶发就不改代码（规则见 docs/e2e测试.md）。

## 9. 工具链约束：格式化只用 oxfmt，独立测试配置的假设要随代码演进

### 现象与根因

1. **格式化器用错**：`npx prettier` 装了全局版（printWidth 80），把 e2e-helpers 整个重排成
   无关 diff。e2e 和前端项目的格式化工具是 oxfmt（`pnpm format:ui`）。
2. **vitest 解析失败**：独立 `vitest.config.ts` 当初假设「被测文件全用相对导入」所以没配
   `@` 别名；后来被测文件引入 `@/utils/logger`，`pnpm test:ui` 立即解析失败——独立测试配置
   与 vite 主配置的隐含假设要对齐（补 `resolve.alias`）。

### 后续参考 / 通用约束

- 前端和 e2e 格式化只用项目 `pnpm format:ui`（oxfmt），不要 npx 其它格式化器。
- 新建「独立于主配置」的配置文件（vitest/tsconfig 分包等）时，逐项核对从主配置继承的假设
  （别名、define、插件）；主配置演进时同步复核。

## 10. 数据到了却一张卡片都不渲染：`shallowRef` 原地改写不触发子组件更新

### 现象

`pnpm dev` 下图像页与提示词页都不出卡片，右侧滚动条也拖不动（不是骨架屏、是彻底空白）。日志显示数据层完全正常：`[list_prompts_page] … items=200 total=525`、`[PagedBlocks:prompt] 块到达 items=200 total=525`、`reload 首屏结束 total=525 items=525 firstItemLoaded=true`；但拖动滚动条只反复出现 `ensureRange {start:0,end:17}`，`start` 恒为 0——说明滚动容器自身高度是 0。

### 根因

`usePagedBlocks` 的 `items` 是 `shallowRef<Array<T | Placeholder>>([])`。原实现把块内容**原地写回同一个数组**（`arr[start+i] = list[i]`、`arr[i] = placeholder()`），随后 `triggerRef(items)`。

`VirtualGrid` 是子组件，`rowCount` / `totalHeight` / `visibleItems` 都由 `props.items` 计算。Vue 判定子组件是否更新看的是 **prop 的引用是否变化**：沿用同一个数组时，即使 `triggerRef` 让父组件重渲染，传下去的还是同一个引用，子组件不会重新计算 → `totalHeight` 恒为 0 → 一张卡片都不渲染、滚动条无可滚动区间。（外层 `watch(pageItems)` 反而会被 `triggerRef` 触发，所以缩略图等副作用照常执行，更容易误判为「数据没问题」。）

### 修复

`usePagedBlocks.ts` 内统一走 `commit(next)`（`items.value = next`）：

- `resize` / `applyBlock` / `clearBlockSlots` / `replaceItem`：先 `items.value.slice()` 造新数组再写，最后 `commit`。
- `reload`：`commit(items.value.map(() => placeholder()))`，保留旧长度以撑住高度（KeepAlive 恢复滚动位置需要）。
- 删除已无用的 `triggerRef` 引入。

### 通用约束

- 用 `shallowRef` 持有「要传给子组件做计算的集合」时，**整体换新数组**（`items.value = next`），不要原地改 + `triggerRef`；就地更新只对「本组件模板直接消费」的浅层数据安全。
- 「数据日志正常但 UI 空白」优先怀疑**引用身份**而非数据内容：对照检查子组件里由 `props.xxx` 派生的计算量（高度、计数、切片）是否恒为初始值。
- 排查这类问题的顺序：先确认数据到达（后端/组合式函数日志）→ 再确认渲染层输入（`props.items.length`）→ 最后才是模板条件与插槽。

## 11. 万级压测图像页「未响应」的 SQL 根因：相关子查询计划退化

### 现象

万级压测（图像 10000 / 提示词 10000）下，切到图像主页窗口立即「未响应」，约 3~7 分钟后自行恢复；
提示词页一切正常。修复计数 SQL 后「初切正常、滚动又卡」——后经日志对账证实是同一个卡死，并非新问题。

### 排查过程

1. CDP（`--remote-debugging-port=9222`）探测：渲染进程 JS 存活（evaluate 16ms 返回、
   `callbacks.size=5` 无挂起 IPC）——排除前端忙循环与 IPC 挂起。
2. `tasklist /V` 两次采样：paim.exe 状态 Not Responding、CPU 时间 10 秒涨 11 秒——
   **宿主主线程忙等**，任务跑完窗口自动恢复（非死锁、非死循环）。
3. 换 `pnpm dev` 复现 + `paim.log` 分步日志：`reloadBlocks 完成 353ms` 之后
   `loadSpecialCounts 完成` 永远没出现——卡死点钉死在 `imageSpecialCounts` 命令内部。
4. SQLite 副本分步计时：带 `JOIN prompts` 的相关子查询 **151 秒**，其余子查询全部毫秒级，
   且索引齐全——查询计划退化，不是缺索引。

### 根因

逐行相关子查询 `(SELECT COUNT(*) FROM prompt_image_relations pir JOIN prompts p
... WHERE pir.image_id = i.id)` 在万级数据上计划退化（release 口径 151s；debug 构建
SQLite 无优化再放大到 406s）。同一个写法在 `prompt_id` 方向实测 24ms、`image_id` 方向 151s——
退化与否取决于 SQLite 的计划选择，**索引齐全不代表安全**。

### 修复

SQL 统一改写：计数用派生表聚合（先 `GROUP BY image_id` 再 `LEFT JOIN`），151s → 90ms，结果一致；
筛选用 `IN` / `NOT IN` 不相关子查询（物化一次），96ms / 33ms。两域（image/prompt）写法完全对称。

### 后续参考 / 通用约束

- **计数/存在性判断禁用相关子查询**：计数用派生表聚合、筛选用 `IN` / `NOT IN`。
- **对称结构优先**：同一逻辑在两个域必须同一种写法；相关子查询「碰巧快」的域也要改，
  对称才能降低认知成本、避免「修一份漏一份」。
- 分步计时定位慢 SQL：把复合查询拆成单分支在只读副本上计时，慢点一目了然；结果一致性用
  新旧写法各跑一遍比对。

## 12. 万级卡死的架构根因：同步 Tauri 命令占主线程

### 现象

与第 11 节同一次事故的另一面：即使 SQL 只有几十毫秒，慢命令执行期间窗口也会冻结——
问题不只在 SQL 本身。

### 根因

Tauri 2 的同步命令（`#[tauri::command]` 不加 `async`）**在宿主主线程执行**，慢 SQL 占住
主线程 → 消息循环停摆 → 窗口「未响应」。这不是偶发事故，是「查询命令无隔离」的必然结果。
「窗口未响应」= 宿主主线程消息循环停摆；渲染进程 JS 存活（CDP 可 evaluate）不能排除宿主卡死。

### 修复

`BkDb` 改 `Arc<Mutex<Connection>>`（拆出 `db::open_connection` 供备份导入取裸连接），
新增 `commands::db_blocking`（async + `spawn_blocking`），图像/提示词两域对称迁移 13 个
查询/自愈命令到后台线程。此后即使 SQL 再退化，窗口也只会「慢」，不会「未响应」。

### 后续参考 / 通用约束

- 同步 Tauri 命令在主线程执行；任何可能超过几十毫秒的命令（查询、文件 IO、缩略图处理）都应
  走 async + spawn_blocking，写命令（用户触发的毫秒级单条操作）可留在主线程。
- 判别「未响应」的顺序：`tasklist /V` 看窗口状态 → 线程级 CPU 采样区分忙等/死锁 →
  分步日志定位到具体命令。
- 改 `BkDb` 包装结构时，新建库取裸连接的路径走 `db::open_connection`，不要再拆 `BkDb.0`。

## 13. 压测诊断三课：paim.log 0 字节、修复未生效的识别、伪故障

### 根因与正确做法

1. **paim.log 0 字节 ≠ 日志系统坏了**：前端 logger 有 `import.meta.env.DEV` 门，
   `tauri build --debug` 产物按生产模式构建（DEV=false），前端日志全 no-op；后端是两套日志——
   终端输出走 `tauri_plugin_log`，写文件的 `infra::logging` 只有 `log_msg` 与备份服务调用。
   压测/性能诊断必须用 `pnpm dev`（DEV=true）跑，分步日志才会落盘。
2. **「修了但没生效」的识别靠对账**：dev 实例没编进修复时，日志冲刷出
   `loadSpecialCounts 完成 405990 ms`（151s × 2.7，SQLite O0）——与修复后的 90ms 对不上。
   「初次切页看起来正常」是渲染进程独立绘画的错觉（主线程其实一直卡着），**日志时间戳/耗时对账是唯一可靠手段**。
3. **分步日志判读**：每个步骤「开始/完成」成对埋点，哪条「开始」没有对应「完成」嫌疑就在哪；
   卡死期间 `log_msg` 同步命令在主线程排队，日志延迟落盘但顺序不变，恢复后集中补写出来。
4. **布局伪故障**：首位组 500 个标签把卡片区整个挤出视口（即使收起筛选区），现象是「只看到标签
   看不到卡片」，极易误判为渲染/数据问题——先确认视口内布局，再怀疑数据层。

### 后续参考 / 通用约束

- 排查「卡在哪一步」最快手段：给关键链路加成对的开始/完成耗时日志（前端日志只在 `pnpm dev`
  下可用，规划复现方式时先确认这一点）。
- CDP 只读观测边界：`window.ipc.postMessage` 不可写不可配置，hook 不可行（非严格模式下赋值
  静默失败）；`__TAURI_INTERNALS__.callbacks.size` 可读，用于判断 IPC 是否挂起。
- 改动「自以为已生效」前，先从日志里找到新旧实现的耗时差异对上号，再下结论。

## 14. 同一逻辑两份实现，修复只改一份：特殊标签「不显示 + 统计一直转」

### 现象

统计弹窗的计数 SQL 修复后，图像页特殊标签仍然不显示，点统计按钮一直「正在统计」——但窗口不再冻结。

### 根因

特殊标签计数有**两份实现**：`statistics_service`（统计弹窗）已修，
`image_service::special_counts`（图像页筛选区）还是慢 SQL。它跑在后台线程（架构改造生效，
所以窗口不冻结了）但要跑两分多钟，期间**占着 BkDb 单连接的锁**，把统计弹窗等后续命令全部堵在队列里。
一个未修的慢命令，表现为另一个功能「卡住」。

### 修复与教训

- 两份实现统一为同构写法；prompt 侧对称核查时又发现同款旧写法（`IMG_COUNT_SQL` 相关子查询），
  一并统一为派生表聚合 + IN/NOT IN 筛选。
- **同一逻辑多份实现是「修一份漏一份」的温床**：发现重复实现时应立即收敛为一份（或至少同构对称），
  并全局搜索同类模式再收尾。
- 排查「A 功能好了、B 功能坏了」：先查两者是否共享资源（这里是单连接的 Mutex 锁），
  慢占用者会把排队者一起拖死。
- 顺手排雷：特殊标签「筛选」（点「未引」/「多引」后分页查询里的逐行过滤）用的是同一个慢模式，
  即使计数修了，筛选一点击照样分钟级——修复要覆盖同一模式的所有使用点，不只看报错的那一处。

## 15. 批量重构的遗漏与类型收尾（BkDb Arc 化 + 卡片字段投影）

### 现象

`BkDb` 从 `Mutex<Connection>` 改 `Arc<Mutex<Connection>>` 后，编译错误分两轮才清完
（第一轮 1 处、第二轮测试里又冒出 2 处）；卡片投影（ImageCard/PromptCard）落地后
vue-tsc 又报 3 处类型失配。

### 根因与教训

1. **grep 单行模式漏跨行调用**：`db::init(...).0\n.into_inner()` 跨行的调用点没被单行 grep
   覆盖。重构后要用能跨行的模式（`rg -U`）或分多个模式扫描（`BkDb(` / `into_inner` / `db::init(`），
   并以编译器为准收尾，不能只扫一遍就宣布清零。
2. **同文件并行 Edit 相互覆盖**：同一轮对同一文件发两个编辑，后一个会覆盖前一个
   （import 被吃掉一次）——同文件的编辑必须串行。
3. **投影类型改造的三类固定漏网点**：① 命令层的同步命令签名（它们不在 `db_blocking`
   批量迁移清单里，最容易漏）；② 事件载荷类型（列表项 = Card、命令返回的全字段 = 原类型，
   update/replaced 两种 emit 要区分）；③ 组件里局部手写的 interface 应换成 bindings 导入，
   手写类型会漂移（漏字段、假字段）。
4. **排序/搜索语义对齐显示口径**：`fileName` 排序原来 `ORDER BY stored_name`（落盘名 `{id}{ext}`），
   用户看到的是 `file_name`——排序键、搜索字段应统一按显示口径，配套迁移部分索引。

### 后续参考 / 通用约束

- 批量重构的收尾顺序：全局多模式扫描 → cargo check/build（编译器找漏网）→ 全量测试；
  「编译通过」不等于「替换完整」（同类型恰好兼容的调用点编译器不报错）。
- 改列表投影类结构时，同步列出「命令签名 / 事件载荷 / 局部 interface / 排序索引」四个检查面逐一过。
