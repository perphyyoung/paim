# e2e 测试说明

基于 Playwright 的端到端测试，连接**真实应用**（真实 Rust 后端 + 真实数据库 + 真实 UI 流程）。
并行模型参考 pm 的 e2e：构建一次产物，每个 worker spawn 自己的应用实例。

## 原理

- 参考项目：[srsholmes/tauri-playwright](https://github.com/srsholmes/tauri-playwright) 的 **cdp 模式**（Windows 专属）。
  WebView2 支持 `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=<port>` 环境变量，
  Playwright 通过 `chromium.connectOverCDP` 直接驱动真实窗口，应用无需加装任何插件或权限。
- 官方文档（v2.tauri.app/develop/tests）的 e2e 方案是 WebDriver（tauri-driver）；Playwright 无官方支持，cdp 模式是社区验证过的替代。

## 并行模型

- `global-setup.ts`：清理残留 paim 进程与 worker 目录，然后 `pnpm tauri build --debug --no-bundle`
  构建一次**带内嵌前端的调试二进制**（等价 pm 的 `pnpm build`），运行期不依赖 vite/devServer。
- `workers: 4` + `fullyParallel: false`：**用例文件间并行、文件内串行**（与 pm 一致）。
  每个 worker 通过 `helpers.ts` 的 `launchApp(workerIndex)` spawn 自己的应用实例：
  - 数据目录 `temp/e2e-w<n>`、WebView2 目录 `temp/wv2-w<n>`（互不冲突，每轮由 globalSetup 清理重建）；
  - CDP 端口按空闲端口动态分配；
  - 用例结束对**自己 spawn 的进程**优雅关闭（WM_CLOSE → 兜底强杀），不影响其他 worker 与 dev 实例。
- `global-teardown.ts` 兜底结束所有 paim 实例（崩溃/超时泄漏的场景）。

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
- **globalSetup / globalTeardown 会结束所有 paim 实例**（含正在使用的 dev 实例），请先保存工作。
- 应用日志写在项目根 `paim.log`，失败排查先看它；用例输出里 `[webview]/[http-error]` 为页面侧日志。

## 约定

- 用例文件按序号命名（`01-*.spec.ts`），文件间并行、文件内串行；涉及数据目录让位的用例（如导入）放最后。
- 引用数据目录内文件前现写一份（`writePng`），不假设旧文件仍在。
- 涉及文件选择的用例复用 `select_images` 测试缝：`launchApp` 已把 mock 路径写入实例环境。
- 用例内采集 webview 控制台与 ≥400 响应日志，输出以 `[webview]/[http-error]` 前缀标识。
