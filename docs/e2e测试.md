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
  每个 worker 通过 `helpers.ts` 的 worker 级 fixture spawn 自己的应用实例：
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
| `PAIM_E2E_MOCK_IMAGE_PATHS` | debug 构建 | `select_images` 命令直接返回其 JSON 路径数组，绕过原生文件对话框（见 `features/image.rs`，参考 pm 的主进程 dialog mock 模式） |

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

## 约定

- 用例文件按序号命名（`01-*.spec.ts`），文件间并行、文件内串行；涉及数据目录让位的用例（如导入）放最后。
- 引用数据目录内文件前现写一份（`writePng`），不假设旧文件仍在；
  上传预览目录按实例隔离（`preview-<PAIM_DATA_DIR 末段>`，见 `db.rs::preview_dir`）。
- 涉及文件选择的用例复用 `select_images` 测试缝：`launchApp` 已把 mock 路径写入实例环境。
- 用例内采集 webview 控制台与 ≥400 响应日志，经 `e2e/e2e-logger.ts` 以 `[webview]/[http-error]`
  等前缀写入 `paim.log`；测试侧新增日志也用它（`e2eLog.debug/info/warn/error`，调用方式与前端 logger 一致），
  不要在 e2e 文件里直接 `console.log` 或另写日志实现。
- **元素定位优先用语义属性**（Playwright 官方推荐）：`getByRole`（角色+名称）、`getByPlaceholder`、
  `getByText` 等，如 `getByRole("button", { name: "上传图像" })`——断言贴近用户视角、抗 UI 重构、
  无需为测试给产品代码加 id。仅当元素没有语义属性可用时才退回 `data-testid`；
  不要用 CSS/XPath 路径选择器（pm 的 `Constants.Ids.*` 是 Electron 时代惯例，Playwright 下不推荐）。
  代价是改用户可见文案需同步改测试，属合理耦合。
