# e2e 测试说明

基于 Playwright 的端到端测试，连接**真实应用**（真实 Rust 后端 + 真实数据库 + 真实 UI 流程）。
并行模型参考 pm 的 e2e：构建一次产物，每个 worker spawn 自己的应用实例。

## 原理

- 参考项目：[srsholmes/tauri-playwright](https://github.com/srsholmes/tauri-playwright) 的 **cdp 模式**（Windows 专属）。
  WebView2 支持 `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=<port>` 环境变量，
  Playwright 通过 `chromium.connectOverCDP` 直接驱动真实窗口，应用无需加装任何插件或权限。
- 官方文档（v2.tauri.app/develop/tests）的 e2e 方案是 WebDriver（tauri-driver）；Playwright 无官方支持，cdp 模式是社区验证过的替代。

## 并行模型

- `global-setup.ts`：`pnpm tauri build --debug --no-bundle` 构建一次**带内嵌前端的调试二进制**
  （等价 pm 的 `pnpm build`），运行期不依赖 vite/devServer。
- `workers: 4` + `fullyParallel: false`：**用例文件间并行、文件内串行**（与 pm 一致）。
  每个 worker 通过 `e2e-helpers.ts` 的 worker 级 fixture spawn 自己的应用实例：
  - 数据目录 `temp/e2e-w<n>`、WebView2 目录 `temp/wv2-w<n>`、上传预览目录 `temp/preview-e2e-w<n>`
    （互不冲突，teardown 时删除数据目录与预览目录）；
  - CDP 端口按空闲端口动态分配；
  - teardown 由 Playwright 保证执行（用例失败/超时也算）：优雅关闭**自己 spawn 的进程**（不影响其他
    worker 与 dev 实例）后删除本轮数据目录。
- 无全局强杀；极端场景（globalTimeout 强杀 worker / 进程崩溃）可能泄漏实例并锁住数据目录，
  下一轮该 worker 会因 paim.db 被占用而启动失败——按报错关闭残留实例即可（`paim.log` 有记录）。

## 测试缝与环境变量

| 环境变量                    | 生效条件   | 作用                                                                                                                          |
| --------------------------- | ---------- | ----------------------------------------------------------------------------------------------------------------------------- |
| `PAIM_DATA_DIR`             | debug 构建 | 数据目录重定向（e2e 指向 `temp/e2e-w<n>`）；同时作为 e2e 实例标识：跳过全局快捷键注册（热键是系统级单例资源）                 |
| `PAIM_E2E_MOCK_IMAGE_PATHS` | debug 构建 | `select_images` 命令直接返回其 JSON 路径数组，绕过原生文件对话框（见 `commands/image.rs`，参考 pm 的主进程 dialog mock 模式） |

原生文件选择对话框无法被任何 webview 自动化工具驱动，所以必须在对话框调用处内建测试缝。
mock 是进程级环境变量，生产环境不存在，不影响发布路径。

## 运行

```bash
pnpm e2e              # 全部用例（文件间并行）
pnpm e2e --grep 上传  # 单个用例
```

- 首次运行 globalSetup 需编译 Rust；globalTimeout 10 分钟。
- 测试期间会弹出多个应用窗口（每 worker 一个），属正常现象。
- e2e 实例与正常 dev 实例（1420）互不干扰；若报「paim.db 被占用」，是上一轮异常退出
  泄漏的实例还开着数据目录，关闭它即可（`paim.log` 有记录）。
- 应用与测试侧日志统一写在项目根 `paim.log`（测试侧带 `[E2E w<n>]` 前缀），失败排查先看它，
  控制台只保留 playwright 自身的用例结果输出；
- 测试侧日志有输出级别阈值，在 `playwright.config.ts` 的 `PAIM_E2E_LOG_LEVEL` 修改：
  默认 `warn`（跑全量只记 pageerror/失败请求/4xx 等异常信号）；
  排查失败时临时改为 `info`/`debug` 重跑，即可看到 `[step]`/`[connect]` 等步骤细节；
- `[connect]` 行（fixture 连上应用时记录，含尝试次数）是排查测试超时的边界标记：
  超时且无该行 → 应用启动/CDP 未就绪（往 webview 启动方向查）；有该行 → 应用正常，
  问题在用例步骤本身（结合 `[step]` 行定位到具体步骤）。尝试次数也直观反映应用启动耗时。

## 复用约定（e2e-helpers.ts）

`e2e/e2e-helpers.ts` 是 e2e 目录的**共用基础设施**（文件名与 `e2e-logger.ts` 对齐，均为 `e2e-` 前缀）。
判定与要求：

| 要求 | 说明 |
| --- | --- |
| spec 只写场景 | **spec 文件里只保留「被测场景的步骤与断言」**；与被测功能无关的样板（怎么点进去、怎么查后端、怎么等 toast）一律下沉到 `e2e-helpers.ts` |
| 下沉判据 | 出现第 2 个用例/文件要用同一段代码 → 下沉。单次使用且紧耦合本场景的步骤（如「右键替换图像」）留在 spec 内 |
| 命名 | 名字要能自解释、带动作对象：`createPromptViaDialog` / `openPromptDetail` / `getItemTagNames` / `waitToastGone` / `uploadImageWithPrompt`。**不要**用 `helper`、`util`、`doIt` 这类无信息量的名字，也不要用缩写 |
| 内容分块 | ① 应用实例 fixture（`test` / `AppHandle`）② 页面操作（导航、建数据、开弹窗、toast 等待）③ 后端直查（`invokeCommand` + 语义化封装）④ PNG 生成。**新增内容按块归位，不随手追加到文件末尾** |
| 调后端命令 | 一律走 `invokeCommand<T>(page, cmd, args?)`，不要在 spec 里重复写 `window.__TAURI_INTERNALS__` 访问样板；常用命令再封一层语义化函数（如 `listPrompts` / `getImagePromptsMap` / `getItemTagNames` / `listTrashedImageIds`） |
| 封装里的断言 | helper 可以做**前置校验断言**（如 `findPromptIdByContent` 找不到就 fail 并带内容），但不要替 spec 做被测行为的断言 |
| 副作用 | 会改数据的 helper（建提示词/上传图像）在文档注释里写明改了什么；等待类 helper（`waitToastGone`）说明为什么必须等（toast 居中且 `pointer-events-auto`，不消失会挡住点击） |

改动 helper 后跑 `npx tsc --noEmit -p e2e`（已纳入 `pnpm check`）；涉及全部用例的改动需完整跑一次 `pnpm e2e`。

## 约定（速查）

| 场景 | 做法 |
| --- | --- |
| 新建用例文件 | **按页面分配、序号命名**（`01-upload-image-page`…）。有独立弹窗即视为独立页面（如「替换图像」在图像详情页，不放上传页文件里）；文件间并行、文件内串行；涉及数据目录让位的用例（如导入）放最后 |
| 打日志 | 用 `e2e-logger.ts` 的 `e2eLog.debug/info/warn/error`（调用方式与前端 logger 一致，自动带 `[E2E w<n>]` 前缀）。**不要**在 e2e 文件里 `console.log` 或另写日志实现 |
| 页面侧诊断 | fixture 已自动采集 webview 控制台消息、失败请求、≥400 响应（`[webview]`/`[pageerror]`/`[req-failed]`/`[http-error]` 前缀写入 paim.log），无需重复采集。失败请求中导航打断的在途请求（`net::ERR_ABORTED`，如用例间复位 reload 时）记 info 级，其余记 error 级 |
| 定位元素 | 语义属性优先：`getByRole("button", { name: "上传图像" })`、`getByPlaceholder`、`getByText`；无语义属性才退 `data-testid`。**禁 CSS/XPath 路径选择器优先**（pm 的 `Constants.Ids.*` 是 Electron 时代惯例，Playwright 下不推荐） |
| 点击卡片 | **点文字层**（`getByText(卡片内容)`），**不要点缩略图 `<img>`**——img 上方盖着文字覆盖层（MediaCard 的 `absolute inset-0`），点 img 会被命中目标检查拦下并重试至超时；点文字会冒泡到卡片根，同样触发打开详情 |
| 打标签 | 两种提交方式都要能用：回车（Enter 命中高亮 → select，未命中 → submit）与点击（点候选项 → select，详情另有「添加」按钮）。候选下拉是 Teleport + fixed `z-[125]`，**会盖住批量弹窗的「确定」按钮**（预期行为，不改布局），所以批量打标签用 `openBatchAddTagDialog` 打开后，一律用回车或点候选项提交，**不要点「确定」** |
| 进批量模式 | `Ctrl + 点击`卡片（普通点击是打开详情）；可复用 `openBatchAddTagDialog` |
| 断言 toast | 用 `.first()`——同一 worker 里前一用例的同文案 toast 可能未消失，直接断言会严格模式冲突（resolved to 2 elements） |
| 用例间状态复位 | page fixture 已内置：**worker 首个用例跳过 reload**（全新实例无残留，且 reload 会打断初始加载的 IPC 请求导致回调失联）；其余用例开始自动 `reload`（上一用例残留的弹窗随之关闭）。用例内不要再自行 `reload` |
| 用新定位 API | 先查类型定义。playwright 1.62 已移除 `getByDisplayValue` 等旧 API；e2e 目录已纳入 `pnpm check` 的类型检查（`tsc --noEmit -p e2e`），方法名写错会在 check 时暴露 |
| 上传/引用文件 | mock 图与数据目录内文件都用 `writePng` **现写一份**（每 worker 独立目录；导入会让数据目录改名，不假设旧文件仍在） |
| 文件选择 | 复用 `select_images` 测试缝（`launchApp` 已把 mock 路径写入实例环境）；替换图像等单选场景需自行校验返回数量 |

## 排查超时

- 先看 `paim.log` 的 `[connect]` 行（fixture 连上应用时记录，含尝试次数）：**超时且无该行** →
  应用启动/CDP 未就绪（往 webview 启动方向查）；**有该行** → 应用正常，问题在用例步骤本身
  （结合 `[step]` 行定位到具体步骤）。尝试次数也直观反映应用启动耗时。
- 控制台只剩 playwright 自身的用例结果输出；测试侧详细日志按上面的级别阈值写进 `paim.log`。
