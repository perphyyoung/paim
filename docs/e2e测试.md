# e2e 测试说明

基于 Playwright 的端到端测试，连接**真实应用**（真实 Rust 后端 + 真实数据库 + 真实 UI 流程）。

## 原理

- 参考项目：[srsholmes/tauri-playwright](https://github.com/srsholmes/tauri-playwright) 的 **cdp 模式**（Windows 专属）。
  WebView2 支持 `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222` 环境变量，
  Playwright 通过 `chromium.connectOverCDP` 直接驱动真实窗口，应用无需加装任何插件或权限。
- 官方文档（v2.tauri.app/develop/tests）的 e2e 方案是 WebDriver（tauri-driver）；Playwright 无官方支持，cdp 模式是社区验证过的替代。

## 两个测试缝（产品代码内建，均有 pm 先例）

| 环境变量                    | 生效条件   | 作用                                                                                                                          |
| --------------------------- | ---------- | ----------------------------------------------------------------------------------------------------------------------------- |
| `PAIM_DATA_DIR`             | debug 构建 | 数据目录重定向到项目根 `temp/e2e-<时间戳>/`，隔离测试数据（见 `db.rs::base_data_dir`）                                        |
| `PAIM_E2E_MOCK_IMAGE_PATHS` | debug 构建 | `select_images` 命令直接返回其 JSON 路径数组，绕过原生文件对话框（见 `features/image.rs`，参考 pm 的主进程 dialog mock 模式） |
| `PAIM_E2E_DATA_DIR`         | 测试进程   | spec 内读取数据目录路径，供文件系统断言（config 写入 process.env）                                                            |
| `PAIM_E2E_PM_BACKUP_ZIP`    | 测试进程   | `pm-backup-import.spec.ts` 使用的真实 pm 备份包路径，缺省指向本机导出文件，不存在则跳过                                       |

原生文件选择对话框无法被任何 webview 自动化工具驱动，所以必须在对话框调用处内建测试缝。
mock 是进程级环境变量，生产环境不存在，不影响发布路径。

## 运行

```bash
pnpm e2e
```

- 运行前关闭正在运行的 dev 实例（1420 端口冲突，webServer 配置为 `reuseExistingServer: false` 会直接报错）。
- 首次运行需要编译 Rust，全局超时 10 分钟。
- 测试期间会弹出应用窗口（最大化），属正常现象。
- 测试数据落在项目根 `temp/e2e-<时间戳>/`，不碰系统临时目录；
  应用下次正常启动清空 `temp` 时自动清理（见 `db.rs::temp_dir`）。
- `pm-backup-import.spec.ts` 需要本机有 pm 导出的备份包（路径见上表），否则该用例跳过。

## 约定

- 新增 e2e 用例放在 `e2e/` 下，文件名 `*.spec.ts`。
- 涉及文件选择的用例复用 `select_images` 测试缝：在 `playwright.config.ts` 的 webServer env 中注入 mock 路径，测试进程内经 `process.env.PAIM_E2E_MOCK_IMAGE_PATHS` 读取同一份值。
- 测试数据只写入临时数据目录，用例内不需要清理。
