# Lessons — 排查记录

## 速查表

| 节 | 主题 | 一句话教训 |
|---|---|---|
| 1 | 冷启动自动弹详情弹窗 / 弹窗子数据为空 | 嵌套 Teleport 的多根组件卸载不可靠，需父级 `v-if` 兜底销毁；被 `v-if` 强制卸载的弹窗，初始化 watch 必须加 `immediate`。 |
| 2 | 「打开本地保存位置」定位失败 | 最终改走 `SHOpenFolderAndSelectItems`：`explorer /select,` 既挑参数形式（拆分 + 全反斜杠），又有新窗口冷启动竞态（首次只到目录、第二次才选中）。 |
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
| 16 | 掉盘后 bindings 静默缺新命令 | 磁盘异常恢复后 cargo 指纹与文件内容都可能部分回滚：`cargo clean -p <主crate>` 强制重编，并用 grep 逐文件核对本轮全部改动；改 tauri-specta 注册后必须 grep 复核 `collect_commands`/`collect_events`。 |
| 17 | 孤儿文件扫描把所有文件误判为孤儿 | 磁盘路径与 DB 路径做差集前，必须统一**基准目录**和**分隔符**；DB 存的是 `images/...`（相对 dataDir + 正斜杠），walk 必须也相对 dataDir 且反斜杠转 `/`，否则差集永远不匹配。 |
| 18 | 全屏查看器关闭后详情弹窗下沿跳动 / 退出多一步「伪全屏」 | 主窗口切原生全屏，退出时的「装饰 → 客户区尺寸」过渡帧躲不掉（不是黑底+顶栏就是下层跳动）；改为查看器独立窗口、主窗口全程不动。 |
| 19 | e2e 假 embedding 自带「假相似」 | 伪向量取值区间非零均值（`[-0.5, 0)`，均值 -0.25）→ 任意两条余弦恒 ~0.75，阈值判定类用例失真；测试替身也要有正确的统计性质（零均值 + 各向异性基线），并加断言锁住。 |
| 20 | e2e 相似度用例的两个交互坑 | 悬浮面板 helper 不幂等（第二次点齿轮被自己的遮罩拦下）、持久化阈值偏好在共用 profile 里跨用例串味；helper 幂等 + 起点显式「重置」+ 收尾清偏好。 |
| 21 | e2e 用例全绿、整轮却 exit 1 | 收尾删数据目录必须与进程退出**串行**（kill 返回 ≠ 句柄释放）；删除可重试、失败改名让位且**绝不抛**（teardown 不该改测试结论）；泄漏的实例目录会被下轮同序号复用旧库，故 globalSetup 起跑前必扫残留。 |
| 22 | 全屏查看按一次 → 跳两格 / 叠加层导航穿透 | 共用胶囊 `NavAndIndex` 已内置 document 键盘导航，查看器又自注册了一份 window 监听，一次按键走两次（≥3 张图才可见）；**能力已内置就别在调用方再实现一份**。同源另一面：document 监听**不区分层级**，叠加层打开时要给被盖住的胶囊传 `disabled`，否则 ←/→ 会把底层条目也切走。 |
| 23 | 打标签后「无标」计数不动 | 特殊标签计数是**内存聚合值**，不随卡片列表更新；打标签两条入口只刷了筛选区（选项还叫 `loadTagFilter`，只含普通标签计数）→ 收成 `reloadTagViews()`（筛选区 + 特殊命中数）注入所有改标签关系的入口（批量 / 拖拽 / 标签管理保存）。 |
| 24 | 嵌套详情里右键没反应 / 点结果无反应或越点越多 | 「层数护栏」做在**入口**上（`!isNested` 把菜单项整条隐藏，弹出空盒子）且嵌套层**漏接 `@open-*`** → 同类结果静默无反应、跨类结果再叠一层。修法是把护栏挪到**跳转目标**（详情嵌套的槽位模型，见 开发经验.md 第 5 节）。 |
| 25 | 多次跳转后嵌套详情变空（「无图像」）、提示词嵌套多一层 | 换槽只改了 props、**实例没重建** → 详情快照仍钉在旧 id（`current` 变 null → 大图落到「无图像」）；且嵌套实例的跨类结果被**自己**接住又在内部叠一层。修法：嵌套槽加 `:key` 随内容重建 + 嵌套实例改为上抛（见 开发经验.md 第 5 节）。 |

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

### 续：修完参数后仍「首次只打开目录、第二次才选中文件」

上面的参数修正只解决了「落到桌面」，还剩一个更隐蔽的问题：目标目录**尚无已打开的 Explorer 窗口**时，窗口冷启动是异步的，`/select` 这条选择命令在视图创建完成前发出就被丢弃，于是只打开目录、不选中；窗口已存在时命令派发给现存视图，所以第二次必中——「第一次到目录、第二次到文件」正是这个竞态的特征。

结论：**不要用命令行协议做「定位并选中」**，改走 Shell 官方入口 `SHOpenFolderAndSelectItems`（Electron 与 tauri-plugin-opener 都用它，后者还在 `ERROR_FILE_NOT_FOUND` 时回退 `ShellExecuteExW`）。要点：

- 调用线程必须先 `CoInitializeEx`；只有返回 `S_OK`（本次完成初始化）才配对 `CoUninitialize`，`S_FALSE`/`RPC_E_CHANGED_MODE` 表示线程上已有别人的初始化，不能卸。
- 进 Shell 之前必须先把路径分隔符归一化为反斜杠：数据库 `relative_path` 存 `/`，`Path::join` 会保留，混用分隔符会让 `ILCreateFromPathW` 直接返回 null（与 `explorer` 回退默认位置同源，坑换了层皮还在）。归一化只做一次，兜底路径复用同一份结果。
- 父目录 PIDL 与文件 PIDL 都要 `ILCreateFromPathW` 生成、`ILFree` 释放；PIDL 只引用路径字符串，两者必须同生命周期（本项目用 `OwnedItemIdList` 同结构体持有）。
- 失败要有兜底（本项目保留 `explorer /select,`，最差仍是打开目录）并记 WARN——这次终于能拿到 HRESULT，不再像 `explorer` 那样返回码恒 0、无法判断成败。

实现见 `src-tauri/src/infra/shell_explorer.rs`：`reveal_in_explorer`（定位并选中，供 `open_image_location`，三处右键共用）与 `open_in_explorer`（只打开目录，供「打开数据目录」）；两者都用 Shell API，失败才回退 `explorer`。

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

`pnpm e2e` 全量跑时 05 号 spec（标签自动补全）间歇失败：上传弹窗点「确定」后不关（textarea 一直
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

## 16. 掉盘恢复后 bindings 静默缺新命令（cargo 指纹损坏 + 文件级部分回滚）

### 现象

新增 `get_log_level`/`set_log_level` 命令并完成全部源码改动后，`pnpm gen:bindings`
成功退出，但 `src/bindings.ts` 里没有新命令，vue-tsc 报 `Property 'getLogLevel' does not exist`；
删掉整个 `target/` 重跑 `pnpm check` 依旧。期间 D: 盘经历了一次掉线-恢复
（写入 ENXIO → 重启后变 RAW → chkdsk 修复）。

### 根因（两个独立问题叠加）

1. **cargo 构建指纹损坏**：掉盘时正逢 link 阶段，恢复后文件的 mtime 被 chkdsk 还原，
   cargo 依据 mtime 误判「产物比源码新」直接复用旧 exe（1.62s 秒过、无 Compiling 行）——
   exe 实际是改动前的二进制，bindings 自然导不出新命令。`cargo clean -p <主crate>` 强制重编即解。
2. **文件级部分回滚**：掉盘时段的编辑被 chkdsk 按文件随机回滚——`logging.rs` 的命令定义存活，
   `lib.rs` 的 `collect_commands`/`collect_events` 注册丢失。编译照常通过（未注册的命令只是
   无人引用），没有任何报错，只有 bindings 缺失这一个静默信号。

### 教训与修复

- **磁盘异常恢复后，构建产物不可信**：先 `cargo clean -p <主crate>` 再 build；验证靠
  `grep 新符号 src/bindings.ts` 有输出，不靠「gen:bindings 退出码 0」。
- **逐文件核对本轮改动**：用 `jj st` / grep 把本轮改过的每个文件过一遍是否仍是预期内容
  ——回滚是按文件咬的，编译通过不代表改动完整。
- **改 tauri-specta 注册后必须 grep 复核** `collect_commands`/`collect_events` 里有新条目——
  注册遗漏是「编译通过但功能不存在」的盲区，与掉盘无关，平时也应作为固定收尾步骤。
- 辅助链路（`gen-bindings.mjs`、e2e 的 exePath）对 target 位置的假设要与环境一致：
  本仓 target 在 workspace 根（根目录 Cargo.toml 是 workspace 根），曾依赖
  `CARGO_TARGET_DIR` 掩盖错误兜底路径，环境变量删除后即暴露。

## 17. 孤儿文件扫描把所有文件误判为孤儿：路径比较的两个必对项

### 现象

设置页「清理孤儿文件」功能，第一次点"扫描"就把所有原图像都标记为孤儿、执行"导出并删除"后全部被导出到目录里——等于**把用户正常数据全搬走了**。

### 排查过程

两次独立 bug，修完一个才暴露下一个：

1. **基准目录错**：`walk_files` 返回相对 `images_dir` 的路径（如 `202401/xxx.png`），DB 存的是相对 `data_dir` 的路径（如 `images/202401/xxx.png`），两边基准不同，差集里永远匹配不上 → 全部成孤儿。pm 的 `getAllFiles(dir, baseDir)` 签名就是双参数，baseDir 明确传 dataDir；我初版写成单参数只传了 imagesDir。
2. **Windows 反斜杠**：改成双参数后仍然全部孤儿——`PathBuf` 在 Windows 上 `to_string_lossy()` 输出 `images\202401\xxx.png`，DB 存的是正斜杠 `images/202401/xxx.png`，字符串比较永远不相等。pm 有显式 `.replace(/\\/g, "/")` 一行，我又漏了。

### 根因

磁盘路径与 DB 路径做集合差集前，**必须统一两个维度**：

- **基准目录**（路径前缀从哪一级开始省）
- **分隔符**（Windows `\` vs 跨平台 `/`）

任一维度不一致，差集结果就是垃圾。这两个都是"看起来理所当然但容易漏"的隐式假设——DB 的存储格式有明确约定（见 image_service::create_image 写的 `format!("images/{yyyymm}/{stored_name}")`），但磁盘遍历返回的路径格式由 OS 和语言库决定，不会自动对齐。

### 修复

`walk_files` 签名改为 `walk_files(dir, base_dir)`（对齐 pm），内部 `path.strip_prefix(base_dir)` 后立即 `replace('\\', "/")`，返回类型从 `Vec<PathBuf>` 改为 `Vec<(String, PathBuf)>`——rel 是已转正斜杠的 String，绝对路径另存一份供后续文件操作使用。

### 后续参考 / 通用约束

- **路径比较先统一格式**：凡是"磁盘 vs DB"或"两个来源的路径字符串做匹配"，先过两个检查：基准目录对不对、分隔符是不是都 `/`。
- **参考项目对齐时签名要抄全**：pm 的 `getAllFiles(dir, baseDir)` 两个参数有明确分工，不能简化成一个；简化往往就是丢掉了关键的格式约定。
- **写完功能先小样本验证**：这次没做——直接在真实数据目录上跑了，代价是把全量数据导出了一遍。新功能涉及文件操作时，先用 `temp/test/` 或临时数据目录验一次再上真实数据。

## 18. 全屏查看器的窗口过渡：下沿跳动 / 退出多一步「伪全屏」

### 现象

在详情弹窗（图像详情 / 提示词详情）里双击大图进全屏查看，点 ✕ 返回后下层详情弹窗的下沿会「跳」一下（先按较大高度排版，再被收缩后的窗口挤回去）；进入全屏时看不出来，只有返回时可见。改成「先还原窗口、再露出弹窗」后跳动消失，但退出又多了一步**伪全屏**：画面是「黑底 + 顶部一栏」，持续约 0.3~0.4s。

### 排查过程

1. 在查看器的进入/退出路径埋点，记录视口与窗口状态（`[Fullscreen] enter-before / enter-after / exit-before / exit+300ms`）。
2. 日志显示窗口几何**精确还原**：`enter-before inner=1920x1057` 与 `exit+300ms inner=1920x1057` 完全一致；两者"不一样"的只是全屏中的 `1920x1080` —— 正好差一个任务栏高度（23px）。
3. 于是把注意力从「窗口没还原」转到「还原过程中谁先露出」：✕ 触发后查看器立即卸载，详情弹窗马上在**仍是 1080 的视口**下渲染，约 370ms 后窗口才收缩到 1057 → 面板重新排版一次，这一次才被看见。
4. 修掉跳动后复测，残留的就是那个「黑底 + 顶栏」帧——它同样是窗口过渡的产物（装饰先回来、客户区尺寸后更新），说明问题不在「谁先露出」的时序编排上。

### 根因

Windows 退出全屏**不是原子操作**：先恢复窗口装饰（标题栏出现），再套用最大化布局（客户区 1080 → 1057），而 WebView 的客户区尺寸更新还要更晚一帧。只要主窗口参与全屏状态，退出时必然有一段「装饰已回来、客户区还是旧尺寸」的过渡期；应用只能选择**这段过渡期显示什么**：显示查看器是「黑底 + 顶栏」，显示下层弹窗是「下沿跳动」，锁面板几何则下层其它元素照样随视口漂移。跳动的放大器是详情弹窗面板用 `h-[85vh]` + 居中排版（85vh 差约 20px，居中后下沿约 10px）。

注：本次 `isFullscreen()` / `isMaximized()` 返回 `null` 不是 API 坏了，而是收敛后的 capabilities 里没有 `core:window:allow-is-fullscreen` / `allow-is-maximized`，IPC 被 ACL 拒绝；诊断改用 `window.innerHeight` + 面板 `getBoundingClientRect()` 即可，不必为此加权限。

### 第一次修复（不彻底）：把「露出」推迟到窗口还原之后

`ImageFullscreenViewer.vue` 改为「`setFullscreen(false)` → 等布局稳定（`waitWindowSettled`：resize 停 80ms、超时 500ms 兜底）→ 再 `emit("close")`」。下层弹窗确实不跳了，但伪全屏帧取而代之——**过渡期本身没消失，只是换了内容**。（该实现与 `windowSettle.ts`、全部临时埋点已在最终修复中删除。）

### 最终修复：查看器独立成窗口，主窗口全程不动

新增 `image-fullscreen` 窗口（`commands/image_fullscreen.rs`）：全屏 + 无装饰 + 隐藏创建，按需创建后复用（`hide` / `show`）。两个详情弹窗不再内嵌查看器，改为 `open_image_fullscreen({ items, index })` 把「列表快照 + 起始索引」交给后端：

| 命令 | 调用方 | 职责 |
| --- | --- | --- |
| `open_image_fullscreen` | 主窗口 | 存载荷 + 通知已有窗口；首次则隐藏创建 |
| `mount_image_fullscreen` | 查看器窗口 | 挂载时取载荷（首次创建路径，事件必丢） |
| `show_image_fullscreen` | 查看器窗口 | 应用载荷并渲染完成后露面 |
| `close_image_fullscreen` | 查看器窗口 | 隐藏窗口 + 聚焦主窗口 |

进入/退出都只是「另一个窗口显示/隐藏」：主窗口与详情弹窗从未被改尺寸，退出即原样露出，**没有任何过渡帧**。前端侧反而更简单：查看器不再需要 `setFullscreen`、等待稳定与 capture 拦截下层键盘，`core:window:allow-set-fullscreen` 也随之从 capabilities 移除。

### 后续参考 / 通用约束

- **「叠一层盖住下层」解决不了窗口级过渡**：装饰与客户区尺寸变化发生在 WebView 之外，网页只能盖住客户区，标题栏那一段永远露着。
- 要「占满整屏 + 退出零过渡」，只能让**主窗口不参与全屏状态**（独立窗口）；同一窗口内做文章只是挑选过渡期显示什么。
- 排查「跳动 / 闪烁」先分清是**几何没还原**还是**时序错位**：对账两个时点的实际尺寸（本处 1057 == 1057）即可快速分开，避免误改窗口还原逻辑。
- 独立窗口的载荷交接必须覆盖两条路：**首次创建**（页面尚未挂载，事件必丢 → 需要可取回的 getter）与**复用窗口**（事件推送）；两条路收敛到同一个 `apply()`。
- 复用窗口要「先应用新载荷并渲染，再 `show()`」，否则会闪一下上一次的内容。
- 跨窗口传参不要依赖 URL query（`WebviewUrl::App` 收的是路径，`?` 能否保留取决于内部解析）：用窗口 label 分流（`src/main.ts` 按 `getCurrentWindow().label`）+ 后端状态中转更稳。

## 19. e2e 假 embedding 自带「假相似」：阈值判定类用例被伪数据带偏

### 现象

新增的相似度 e2e（`e2e/11`）在**默认阈值 0.5** 下两栏就全命中（卡片角标如 `0.753`）；把阈值降到 0 之后，反而又拿不到结果。

### 根因

`MockEmbedder::pseudo` 的取值区间是 `[-0.5, 0)`（均值 -0.25，**非零均值**）→ 每个分量都偏负，
任意两条伪向量余弦 ≈ `E[v]² / (E[v]² + Var[v]) ≈ 0.75`，假实现自带一个「全都相似」的基线；
换成零均值噪声后又有另一半组合是**负**余弦，`score >= min_score`（阈值 0）照样过滤。

### 修复

改成**零均值噪声 + 0.1 的各向异性基线**：任意两条 ≈0.1（恒正、远低于默认阈值 0.5），
与真实 embedding 空间的各向异性一致 —— **阈值 0 → 全命中、默认 0.5 → 全过滤**；单测加断言锁住该性质。

### 后续参考 / 通用约束

- **测试替身光「确定性」不够，分布（均值 / 方差 / 相似度基线）同样决定用例结论**。
- 失败先读 `test-results/*/error-context.md` 的页面快照核对 UI 实际值（角标、输入框），再怀疑断言与数据。
- 阈值 / 排序类用例让基线相似度与阈值**分居两侧**（本例 0.1 vs 0.5），别让用例「刚好压线」。

## 20. e2e 相似度用例的两个交互坑：helper 不幂等 + 持久化偏好跨用例串味

### 现象与根因

1. 连续两次「跑相似度索引」时，第二次点设置齿轮被**自己的遮罩**拦下（`intercepts pointer events`）——`openSettings` 每次都点齿轮，但设置是**悬浮面板**，第一次调用后它一直开着。
2. 上一轮中断的用例把阈值 0 留在 localStorage，新用例一开就全命中 —— 阈值是**持久化偏好**，而同 worker 的 spec 共用同一 WebView2 profile（跨文件、跨轮次保留），中断的用例不会执行收尾清理。

### 修复

`openSettings` 幂等（面板已可见就复用）；用例起点用 `resetBothPanesAndRequery(modal)` 拉回确定默认，收尾用 `clearSimilarityThresholds(page)` 清偏好（与 `setListBlockSize` / `clearListBlockSize` 同一约定）。

### 后续参考 / 通用约束

- 悬浮面板类 helper 一律做成**幂等**（先可见再点开），否则「连续调用同一 helper」的用例必踩遮罩。
- 凡被用例改动的**持久化偏好**（localStorage）：起点显式重置或收尾清理 —— 跨文件共用 profile 时尤其重要。
- 相似度功能的完整经验见 [开发经验.md](./开发经验.md) 第 4 节；同类教训见 §5。

## 21. e2e 收尾「kill 即删」：用例全绿的一轮被 teardown 判失败（Windows 句柄释放时序）

### 现象与根因

`pnpm e2e` 29 个用例全部通过，收尾却报 `1 error was not a part of any test` + `[ELIFECYCLE] exit 1`：
Playwright 把该 worker 判失败，报错落在 `e2e-helpers.ts::disposeApp` 的 `fs.rmSync(app.dataDir)`。

- 这是**竞态**，不是某用例的逻辑问题：同一轮日志里 w0 / w3 的**最后一个实例**没有 `[app] 进程退出` 行，
  且 `temp/e2e-w0-2`、`temp/e2e-w3-2`（各含 preview 目录）**留在磁盘上**；w1（最后一个实例走
  `restartApp`，那里等了退出 + 2s）与 w2（恰好死在 rm 之前）没踩中。
- 根因在 `closeApp`：`taskkill /PID` → 固定 `sleep(1500)` → `taskkill /F /T` **发出即返回**，
  从不等待进程真正退出；紧接着 `disposeApp` 就删目录。Windows 上 kill 返回 ≠ 句柄已释放
  （`paim.db`、缩略图、asset 协议读过的文件），而 `fs.rmSync` 的 `force: true` 只忽略 ENOENT、
  **不重试锁占用** → EBUSY 直接抛 → 整个 worker 判失败（用例结论被环境噪声盖掉）。

### 修复

1. 新增 `waitForExit(child, ms)`（`exitCode` 已置位即返回；否则听 `exit` 事件，定时器 `unref` 不拖住退出）；
   `closeApp` 改为 `taskkill /PID` → 等 3s → 未退出再 `/F /T` → 等 10s → 仍未退出记 `[app] …` error；
2. `disposeApp`：关闭 → **等退出** → `removeDirBestEffort()`（`rmSync` 带 `maxRetries/retryDelay`，
   仍失败则改名为 `*-stale-<ts>` 让出目录名），**绝不抛**；
3. `globalSetup` 起跑前扫掉残留的 `temp/e2e-*` / `preview-e2e-*` / `*-stale-*` —— 因为「本 worker 内序号」
   每轮从 0 计数，上一轮泄漏的目录会被本轮同序号**复用旧库**（库里已有同 md5 的图 → 用例被判重复导入），
   这个隐性污染比直接报错更难查。

### 后续参考 / 通用约束

- **进程生命周期类 teardown 一律「等退出再动它的文件」**：kill/close 都是异步生效的，固定 sleep 是猜。
- **清理失败不得改变测试结论**：teardown 只记日志（改名让位 + 交由下轮 globalSetup 扫尾）。
- Windows 上目录内含被占文件**不能删、但能改名**——「改名让位」比无限重试更可靠。
- 同类教训：§6（vite 监听句柄挡住数据目录改名）、§8（e2e 全量偶发「页面消失」）。

## 22. 全屏查看按一次 → 跳两格：键盘导航两份实现（共用胶囊 + 局部监听）

### 现象与根因

在提示词详情用「从外界导入图像」把关联图像撑到 3 张以上后，双击进全屏查看按一次 → 索引从 1 / 3 直接到 3 / 3（跳项）。

- `NavAndIndex`（共用胶囊）**内置**键盘导航：`document` 监听 ←/→、Home/End，派发 first/prev/next/last（详情弹窗与查看器共用同一份，设计注释写明「不依赖焦点」）；
- `ImageFullscreenViewer` 又自己注册了一份 `window` 监听，直接调 `nav(±1)`；
- 一次按键两个监听都收到（事件冒泡依次经过 `document` 与 `window`），各走一次 → 索引 **+2**。
- 为什么长期没暴露：1 张图时箭头禁用；2 张图时 +2 被钳位到末项，看起来和正常翻页一样；Home/End 重复触发是幂等的——**≥3 张图才看得见**。
- 定位手法：先写用例把两个猜想分开证伪（`e2e/12`）——「双击第 N 张应以第 N 张开场」的用例通过、「按一次只走一格」的用例失败 → 直接指到键盘双监听，而不是 payload 索引算错。

### 修复

删掉 `ImageFullscreenViewer` 里的 `window` keydown（含 Home/End 分支），键盘导航统一由 `NavAndIndex` 负责（查看器模板已把它的四个事件接到本组件的 `nav`/`goFirst`/`goLast`）。

### 同源的另一面：胶囊是 document 级，不区分层级

顺着「谁还注册了导航键」全库排查后，**「同一视图内同键注册两次」只剩查看器这一处**（其余 keydown 是 Ctrl+F / F5 / Ctrl+P·I·T·A / Esc / Alt+I / 输入框内键，与 ←/→/Home/End 不重叠）。但胶囊的 `document` 监听是**挂载即生效**，于是暴露出第二类问题——**穿透**：详情弹窗上再叠一层（嵌套图像详情 / 图像导入选择器 / 相似结果页 / 确认框）时，被盖住的底层胶囊照旧响应 ←/→/Home/End。嵌套层的 order 恒为 1 条（`:order="[xxx[0]?.id ?? '']"`），所以**不会**出现「+2 跳项」，但底层条目会被悄悄切走（关掉嵌套层才发现在另一条上）；相似结果页则是被 `watch(current.id)` 收起——那其实是在兜底这个泄漏。

修复与两个详情弹窗既有的 Ctrl+F `guard` 同思路、一处收口：

- `NavAndIndex` 增可选 `disabled`：`onNavKeydown` 首行 `if (props.disabled) return;`（不 `preventDefault`，按键留给上层）；
- 两个详情弹窗把「上层叠加层是否打开」收敛成**一个 computed**（`overlayOpen`），既给 `guard`、也给 `:disabled="overlayOpen"`，避免两处条件各自漂移；
- 回归用例两条（`e2e/12`）：分别覆盖两个弹窗，断言「叠加层开着时按 ←/→，底层索引文案不变」；实现后临时停用守卫再跑一次，确认两条必红（否则用例无意义）。

> 另注：胶囊对 `INPUT/TEXTAREA/SELECT` 放行是既有设计（编辑/搜索时不导航），本轮未改。

### 后续参考 / 通用约束

- **共用组件已内置的能力，调用方不要再实现一份**：`NavAndIndex` 的键盘导航是内置设计，调用方只接事件（图像详情 / 提示词详情就是这么用的，只有查看器多写了一份）。
- **document/window 级全局监听要显式处理层级**：挂载即生效、不区分谁在最上面；有叠加层的组件必须提供「停用」入口，并在每处叠加关系上接线（参考既有的 Ctrl+F `guard` 写法）。
- 这类问题随**列表长度**开关：写用例要凑到能触发的规模（本例 ≥3），否则「通过」是假的；跨用例共用实例时，**不要假设总数**（用「前后不变」比较，别写死 `/ 2`）。
- 排查时优先「两个猜想各写一条断言」，先让用例把 payload 侧与交互侧分开证伪，再动代码；修完再临时停用守卫跑一次，证明用例真的能红。

## 23. 特殊标签计数是内存值：改了标签关系却没重拉，「无标」chip 停在旧数字

### 现象与根因

提示词主页给一条「无标」提示词打上标签（批量工具栏、或把筛选区标签拖到卡片）后，左栏「无标」chip 的数字没变——该消失时也仍然在。图像主页同理。

- 特殊标签命中数（`收藏` / `无标` / `单语` / `未引` …）由后端聚合、主页拉到 `specialTagsCounts`（普通 `ref`）后**不会随卡片列表自动更新**；而它随「标签关系」变化：给无标条目打标签 → 无标 -1。
- 打标签的两条入口此前只刷了筛选区：`useBatchTagAdd` / `useCardTagAdd` 的选项名叫 `loadTagFilter`，成功后只调它（`getTagData` + `getTagsMap`，含普通标签计数与标签源），**没有**调 `loadSpecialTagsCounts`。标签管理保存（删除标签会让条目变「无标」）也只调了 `loadTagFilter`。

### 修复

- 两个主页各加 `reloadTagViews()`（`Promise.all([loadTagFilter(), loadSpecialTagsCounts()])`），注入 `useBatchTagAdd` / `useCardTagAdd`；两个组合式函数的选项随之改名 `reloadTagViews`，类型注释写明「筛选区 **和** 特殊命中数都要刷」——只刷一半正是本次问题的形状，选项名必须把契约说全。
- `onTagManagerSaved` 同样改走它。
- 回归用例 `e2e/13`（提示词主页批量、图像主页拖拽各一条）：断言「最后一条无标条目被打上标签后，无标 chip 消失」。chip 只在计数 > 0 时渲染，比读数字稳，而且同时证明了落库与主页刷新两件事。

### 顺带审计（同一类：改了数据没重拉计数）

- **已覆盖**（都经过 `loadPrompts` / `loadImages`，其中已含本计数）：详情关窗（有改动时）、上传完成、切页脏标记、批量收藏、批量删除、单张删除、回收站恢复 / 清空 / 彻底删除（彻底删除的条目本就 `is_deleted = 1`，不进计数）。
- **卡片上的单张收藏**（`useItemToggle.toggleOne`）原先只写回列表项，没重拉特殊计数 → 「收藏」chip 停在旧值。已补：`useItemToggle` 增 `afterToggle` 回调，两个主页传 `loadSpecialTagsCounts`（卡片上只有收藏这一项切换，所以只重拉该计数、不重拉整个筛选区）；`afterToggle` 自身失败单独吞掉，避免切换已成功却报「更新失败」。

### 后续参考 / 通用约束

- 「内存聚合值 + 多个写入口」是漏刷的温床：**凡改数据的地方都要问一句「哪些计数依赖它」**，并把刷新收成一个具名函数（`reloadTagViews`）供各入口复用，别让每个入口各自记得刷一半。
- 组合式函数的回调选项要按**契约**命名：`loadTagFilter` 这种名字会诱导调用方只传「筛选区刷新」，改名 `reloadTagViews` 后契约写进类型注释，接错一眼可见。
- 计数类断言优先用「chip 的出现 / 消失」（计数 > 0 才渲染），而不是读数字：既不受跨用例数据规模影响，也顺带证明主页确实刷新过。

## 24. 多层详情跳转的静默失效：入口被条件隐藏 + 嵌套层漏接线

### 现象与根因

图像详情 → 相似结果页 → 点提示词结果 → 打开的提示词详情是**嵌套态**（父级传了 `is-nested`），在它的**内容上右键没有任何菜单** —— 不是点了没反应，而是**根本没有菜单项可选**。

两处叠加：

- 内容右键的菜单项写着 `v-if="similarityEnabled && !isNested"`：嵌套态下**一个子项都不渲染**，`ContextMenu` 弹出的是**空盒子**（CSS 上就是一块空白浮层，看起来与「右键没绑定」一模一样）；
- 同一层嵌套还**缺 `@open-*` 接线**：`ImageDetailModal` 里那处嵌套 `PromptDetailModal` 只接了 `@close`/`@updated`，`PromptDetailModal` 里那处嵌套 `ImageDetailModal` 只接了 `@close`/`@replaced`/`@safe-synced`。**所以就算把菜单放开**：点同类结果 → **静默无反应**（emit 无人接）；点跨类结果 → **再叠一层**（无上限）。

### 为什么难发现

- 症状是「没有反应」而不是报错：空 `ContextMenu` 与「没绑右键」在界面上无法区分，只能靠读条件表达式定位。
- **层数护栏做在了入口上**（`!isNested`）→ 功能不是被限制，而是被**隐藏**；后来想放开时还得先发现嵌套层缺接线，否则放开的只是入口、链路仍是断的。
- 要 3 层才看得出危害：2 层时一切正常（首轮实现只覆盖到 2 层，所以看起来是好的）；「越点越多」也只有连点几次才暴露。

### 修法与验证

修法是把护栏从入口挪到**跳转目标**上（嵌套详情的「槽位模型」）—— 模型、规则与接线清单见 [开发经验.md](./开发经验.md) 第 5 节。
回归用例 `e2e/11` 第 3 条（嵌套层内容右键可见 → 跨类结果落本层槽 → 同类只替换 → 逐层关闭后底部原样），**修复前必红**（右键后等不到菜单项）。
（后续已重构成显式「嵌套详情栈」：`useNestedDetails` + `<NestedDetailSlots>`，`isNested` 上抛分支与接线不再散在两个弹窗里。）

### 后续参考 / 通用约束

- **共享能力（结果跳转 / 快捷键 / 右键菜单）要在每一层各接线一次**，漏一层的表现往往是静默无效（与 §22 同源）。
- **别用「条件隐藏入口」当护栏**：它会把 bug 伪装成「没这功能」；要么把限制做在跳转目标上，要么置灰 + `title` 说明理由（`禁止二级跳转` 那种写法是可接受的）。
- 这类交互**先用 e2e 钉住再改**：断言写成「层数 + 逐层关闭后关键内容仍在」，不要猜 DOM 顺序。

## 25. 换槽后嵌套详情「停在旧 id」：大图变「无图像」，同类结果又叠一层

### 现象与根因

相似度结果里**多次跳转**后，嵌套的图像详情界面变空（大图位置显示「无图像」）。同一轮还夹着第二个症状：在嵌套图像详情里点提示词结果，会**再多叠一层**（嵌套提示词出现两个）。

- 第一处：详情按「进入那一刻的顺序快照」定位当前项（`useDetailSnapshot.init()` 只在初始化时把 `currentId` 固定住）。而**换槽只是改 props**（`images` / `order` 换成新内容），嵌套槽没有 `v-if` / `key` → 实例不重建 → `currentId` 仍是旧 id → `current` 变 `null` → `origSrc` 被清空 → 模板落到 `<p>无图像</p>`。用户看到「跳几次后空了」，真实状态其实是**实例没换、数据换了**。
- 第二处：在**嵌套**图像详情里点提示词结果时，被接住的是它**自己的** `@open-prompt`（结果页挂在它自己身上）→ 它在内部再开一层提示词 → 链变 4 层，且嵌套提示词有两个（违反「各自的嵌套层最多一个」）。

### 为什么难发现

- 与操作路径强相关：单次跳转一切正常，要「多次跳转」才复现。
- 首版 e2e 只断言**层数**（`toHaveCount`）——层数是对的、内容是空的，于是用例绿而界面空（典型的「断言选错了信号」）。
- 「数据换了」在 Vue 里是隐性的：props 变了但组件不重建，依赖 `watch(open / initialIndex)` 的初始化逻辑不会重跑。

### 修法与验证

- 两个嵌套槽加 `:key`（跟槽内容 id 走）→ 替换时重建实例，`init()` 重新定位，旧实例的本地状态一并清掉；
- **嵌套实例**的跨类结果改为**上抛**（`ImageDetailModal.onOpenSimilarPrompt` 在 `props.isNested` 时 `emit("open-prompt")`），由宿主替换它那一层 —— 规则见 [开发经验.md](./开发经验.md) 第 5 节；
- 回归用例 `e2e/11` 第 3 条加严：换槽后断言**大图真的换了**（无「无图像」且 `src` 变成另一张）与**提示词槽内容换新**，修复前两处都红。
- （后续重构成「嵌套详情栈」：`:key`、上抛与实例重建都收敛进 `NestedDetailSlots` / `useNestedDetails`，弹窗侧只剩「调 `openNested(kind, id)`」，此类漏改不再可能发生。）

### 后续参考 / 通用约束

- **「改 props」不等于「换实例」**：靠「打开那一刻」初始化的状态（详情快照、缓存、本地草稿）必须随内容变化重建（`:key`），否则界面停在旧 id / 旧内容上。
- 断言「槽位替换」不要只看层数：**层数对 ≠ 内容对**，要断言内容真的换新（本例：`src` 变化、旧文案消失）。
- 结果页挂在每个详情实例上 → **嵌套实例必须自己决定「用本层槽还是上抛」**，这是槽位模型最容易漏的一条。
