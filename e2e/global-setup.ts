/// 全局设置：为整轮 e2e 构建一次带内嵌前端的调试二进制（等价 pm 的 pnpm build）。
/// 之后每个 worker 直接 spawn 该 exe（自起实例），不再需要 vite/devServer。
/// 先清掉上一轮可能残留的实例与 worker 目录（崩溃/超时泄漏的 paim.exe
/// 会锁住数据目录，导致本轮启动清理与数据库打开失败）。
import fs from "node:fs";
import { execSync } from "node:child_process";
import { join } from "node:path";

export default async function globalSetup() {
  try {
    execSync("taskkill /F /T /IM paim.exe", { stdio: "ignore" });
  } catch {
    // 无残留实例
  }
  const tempDir = join(import.meta.dirname, "..", "temp");
  if (fs.existsSync(tempDir)) {
    for (const name of fs.readdirSync(tempDir)) {
      if (name.startsWith("e2e-") || name.startsWith("wv2-")) {
        fs.rmSync(join(tempDir, name), { recursive: true, force: true });
      }
    }
  }

  console.log("[global-setup] 构建调试二进制（tauri build --debug --no-bundle）...");
  execSync("pnpm tauri build --debug --no-bundle", {
    cwd: join(import.meta.dirname, ".."),
    stdio: "inherit",
  });
}
