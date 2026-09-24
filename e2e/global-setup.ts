/// 全局设置：整轮 e2e 前做两件事——
/// 1. 清掉上一轮泄漏的实例目录（数据目录 / 预览目录 / 改名让位留下的 `*-stale-*`）；
/// 2. 构建一次带内嵌前端的调试二进制（等价 pm 的 pnpm build）。
/// 之后每个 worker 直接 spawn 该 exe（自起实例），不再需要 vite/devServer。
/// 构建输出不进控制台，成败记录在 paim.log（失败时错误信息含编译输出尾部）。
import fs from "node:fs";
import { execSync } from "node:child_process";
import { join } from "node:path";
import { e2eLog } from "./e2e-logger";

/// 清理上一轮可能泄漏的实例目录。
/// 为什么必须做：目录名是 `temp/e2e-w<worker>-<序号>`，而序号每轮从 0 重新计数；
/// 上一轮若某实例的目录没删掉（进程句柄未释放 → 见 e2e-helpers 的 removeDirBestEffort），
/// 本轮同 worker 跑到相同序号就会**复用旧库**——库里已有同 md5 的图，用例会被判「重复导入」，
/// 表现为难查的偶发失败（比直接报错更贵）。`wv2-w<n>` 是按 worker 长期复用的 profile，不动。
function sweepLeakedDirs(): void {
  const temp = join(import.meta.dirname, "..", "temp");
  if (!fs.existsSync(temp)) return;
  const leaked = fs
    .readdirSync(temp)
    .filter((name) => /^(preview-)?e2e-w\d+-\d+$/.test(name) || /-stale-\d+$/.test(name));
  for (const name of leaked) {
    const dir = join(temp, name);
    try {
      fs.rmSync(dir, { recursive: true, force: true, maxRetries: 10, retryDelay: 200 });
      e2eLog.warn(`[global-setup] 清理上一轮残留的实例目录 ${name}`);
    } catch (e) {
      e2eLog.warn(`[global-setup] 残留目录清理失败（本轮该序号可能复用旧数据）：${name} — ${e}`);
    }
  }
}

export default async function globalSetup() {
  e2eLog.info("[global-setup] 清理上一轮残留的实例目录");
  sweepLeakedDirs();
  e2eLog.info("[global-setup] 构建调试二进制（tauri build --debug --no-bundle）");
  execSync("pnpm tauri build --debug --no-bundle", {
    cwd: join(import.meta.dirname, ".."),
    stdio: "ignore",
  });
  e2eLog.info("[global-setup] 构建完成");
}
