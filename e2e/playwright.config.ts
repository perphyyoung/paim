/**
 * e2e 测试配置：CDP 模式连接真实 Tauri 应用（参考 srsholmes/tauri-playwright 的 cdp 思路）。
 *
 * 原理：WebView2 支持 WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS 环境变量，注入
 * --remote-debugging-port 后，Playwright 可通过 chromium.connectOverCDP 驱动真实窗口，
 * 无需给应用加装任何插件或能力。测试走真实 Rust 后端与真实数据库。
 *
 * 对话框 mock（参考 pm 的主进程 dialog mock 模式）：原生文件对话框无法被 Playwright
 * 驱动，应用在 select_images 命令内建了测试缝——debug 构建且设置
 * PAIM_E2E_MOCK_IMAGE_PATHS 环境变量时直接返回其路径。这里生成临时 png 并注入。
 *
 * 数据隔离：webServer 启动应用时注入 PAIM_DATA_DIR，指向项目根 temp/e2e（见
 * db.rs 的 base_data_dir / temp_dir），并给 WebView2 独立的用户数据目录；应用下次
 * 正常启动清空 temp 时自动清理，每轮 e2e 都是全新数据目录，不污染 paim-data。
 *
 * 运行前提：先关闭正在运行的 dev 实例（1420 端口冲突）。
 */
import fs from "node:fs";
import path from "node:path";
import { defineConfig } from "@playwright/test";
import { writePng } from "./helpers";

// 路径必须确定性固定：配置模块会被 playwright 的 runner 与 worker 各求值一次，
// 含随机后缀的路径会在两侧分叉。temp 与数据目录必须分离（pm 备份导入会把
// 整个数据目录改名让位）；应用下次正常启动清空 temp 时自动清理。
const dataDir = path.join(import.meta.dirname, "..", "temp", "e2e");
const webview2Dir = path.join(import.meta.dirname, "..", "temp", "wv2");
fs.mkdirSync(dataDir, { recursive: true });

const mockImagePath = path.join(dataDir, "e2e-upload.png");
writePng(mockImagePath);
const mockImagePaths = JSON.stringify([mockImagePath]);
// 同时写入测试进程环境：webServer.env 只传给应用进程，spec 需读取同一份值
process.env.PAIM_E2E_MOCK_IMAGE_PATHS = mockImagePaths;

export default defineConfig({
  testDir: import.meta.dirname,
  // 覆盖应用启动等待（CDP 连接重试）与上传流程；globalTimeout 覆盖首次编译耗时
  timeout: 120_000,
  globalTimeout: 600_000,
  workers: 1,
  fullyParallel: false,
  reporter: "list",
  globalTeardown: "./global-teardown.ts",
  webServer: {
    // e2e 用独立端口（vite 1430 + CDP 9223，见 tauri.e2e.conf.json / VITE_PORT），
    // 与正常开发的 1420 实例互不干扰
    command: "pnpm tauri dev --no-watch --config src-tauri/tauri.e2e.conf.json",
    // webServer 默认以配置文件所在目录（e2e/）为 cwd，tauri CLI 需在项目根运行
    cwd: path.join(import.meta.dirname, ".."),
    url: "http://localhost:1430",
    reuseExistingServer: false,
    env: {
      VITE_PORT: "1430",
      PAIM_DATA_DIR: dataDir,
      PAIM_E2E_MOCK_IMAGE_PATHS: mockImagePaths,
      // WebView2 用户数据目录必须也在数据目录之外：运行期全程持有句柄，
      // 放进数据目录会让「导入时数据目录整体改名让位」报 os error 5
      WEBVIEW2_USER_DATA_FOLDER: webview2Dir,
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: "--remote-debugging-port=9223",
    },
  },
});
