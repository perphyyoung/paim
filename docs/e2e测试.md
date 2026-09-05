# e2e 测试说明

基于 Playwright 的端到端测试，连接**真实应用**（真实 Rust 后端 + 真实数据库 + 真实 UI 流程）。

## 原理

- 参考项目：[srsholmes/tauri-playwright](https://github.com/srsholmes/tauri-playwright) 的 **cdp 模式**（Windows 专属）。
  WebView2 支持 `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9223` 环境变量，
  Playwright 通过 `chromium.connectOverCDP` 直接驱动真实窗口，应用无需加装任何插件或权限。
- 官方文档（v2.tauri.app/develop/tests）的 e2e 方案是 WebDriver（tauri-driver）；Playwright 无官方支持，cdp 模式是社区验证过的替代。

## 测试缝与环境变量

| 环境变量                    | 生效条件   | 作用                                                                                                                          |
| --------------------------- | ---------- | ----------------------------------------------------------------------------------------------------------------------------- |
| `PAIM_DATA_DIR`             | debug 构建 | 数据目录重定向到项目根 `temp/e2e`，隔离测试数据（见 `db.rs::base_data_dir`）                                                  |
| `PAIM_E2E_MOCK_IMAGE_PATHS` | debug 构建 | `select_images` 命令直接返回其 JSON 路径数组，绕过原生文件对话框（见 `features/image.rs`，参考 pm 的主进程 dialog mock 模式） |

原生文件选择对话框无法被任何 webview 自动化工具驱动，所以必须在对话框调用处内建测试缝。
mock 是进程级环境变量，生产环境不存在，不影响发布路径。

## 运行

```bash
pnpm e2e              # 全部用例
pnpm e2e --grep 上传  # 单个用例
```

- e2e 使用**独立端口**（vite 1430 + CDP 9223，见 `tauri.e2e.conf.json` / `vite.config.ts` 的 `VITE_PORT`），
  可与正常开发的 1420 实例并存。
- 首次运行需要编译 Rust，全局超时 10 分钟。
- 测试期间会弹出应用窗口（最大化），属正常现象。
- **结束时会结束所有 paim 实例**（`global-teardown.ts`；playwright 只杀 tauri CLI，
  孙进程 paim.exe 会残留并污染下一轮，故统一清理）。正在使用的 dev 实例请先保存工作。
- 测试数据落在项目根 `temp/e2e`，每轮全新（应用启动清空 temp）；应用日志写在项目根 `paim.log`，失败排查先看它。

## 约定

- 用例文件按序号命名（`01-*.spec.ts`），控制执行顺序：涉及数据目录让位的用例（如导入）放最后。
- 引用数据目录内文件前现写一份，不假设配置期写入的文件仍存在（导入让位会改名整个数据目录）。
- 涉及文件选择的用例复用 `select_images` 测试缝：在 `playwright.config.ts` 注入 mock 路径，
  测试进程内经 `process.env.PAIM_E2E_MOCK_IMAGE_PATHS` 读取同一份值。
- 用例内采集 webview 控制台与 ≥400 响应日志，输出以 `[webview]/[http-error]` 前缀标识。
