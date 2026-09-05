/**
 * e2e 测试配置：CDP 模式连接真实 Tauri 应用（参考 srsholmes/tauri-playwright 的 cdp 思路）。
 *
 * 并行模型（参考 pm 的 e2e）：globalSetup 构建一次带内嵌前端的调试二进制，
 * 每个 worker spawn 自己的应用实例（独立数据目录/WebView2 目录/CDP 端口），
 * 用例结束优雅关闭自己的实例——worker 之间、与正常开发的 1420 实例之间互不干扰。
 * 对话框 mock 与数据目录隔离见 helpers.ts / docs/e2e测试.md。
 *
 * 运行前提：关闭正在运行的 dev 实例（globalSetup 会先清理残留的 paim 进程，
 * teardown 阶段不杀任何进程）。
 */
import { defineConfig } from "@playwright/test";

// e2e 测试侧日志的输出级别（写入 paim.log 的阈值）：
// 默认 warn——跑全量用例只记异常信号（pageerror/失败请求/4xx 等）；
// 排查失败时临时改为 info 或 debug 重跑，即可看到 [step]/[connect] 等步骤细节
process.env.PAIM_E2E_LOG_LEVEL ??= "warn";

export default defineConfig({
  testDir: import.meta.dirname,
  // 覆盖应用启动等待（CDP 连接重试）；globalTimeout 覆盖 globalSetup 构建耗时
  timeout: 120_000,
  globalTimeout: 600_000,
  // 文件间并行（每 worker 一个应用实例），文件内串行（与 pm 一致）
  fullyParallel: false,
  workers: 4,
  reporter: "list",
  globalSetup: "./global-setup.ts",
});
