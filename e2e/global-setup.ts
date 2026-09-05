/// 全局设置：为整轮 e2e 构建一次带内嵌前端的调试二进制（等价 pm 的 pnpm build）。
/// 之后每个 worker 直接 spawn 该 exe（自起实例），不再需要 vite/devServer。
/// 构建输出不进控制台，成败记录在 paim.log（失败时错误信息含编译输出尾部）。
import { execSync } from "node:child_process";
import { join } from "node:path";
import { e2eLog } from "./e2e-logger";

export default async function globalSetup() {
  e2eLog.info("[global-setup] 构建调试二进制（tauri build --debug --no-bundle）");
  execSync("pnpm tauri build --debug --no-bundle", {
    cwd: join(import.meta.dirname, ".."),
    stdio: "ignore",
  });
  e2eLog.info("[global-setup] 构建完成");
}
