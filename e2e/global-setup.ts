/// 全局设置：为整轮 e2e 构建一次带内嵌前端的调试二进制（等价 pm 的 pnpm build）。
/// 之后每个 worker 直接 spawn 该 exe（自起实例），不再需要 vite/devServer。
import { execSync } from "node:child_process";
import { join } from "node:path";

export default async function globalSetup() {
  console.log("[global-setup] 构建调试二进制（tauri build --debug --no-bundle）...");
  execSync("pnpm tauri build --debug --no-bundle", {
    cwd: join(import.meta.dirname, ".."),
    stdio: "inherit",
  });
}
