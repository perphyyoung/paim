/// 全局设置：整轮 e2e 前做两件事——
/// 1. 清掉上一轮泄漏的实例目录（数据目录 / 预览目录 / 改名让位留下的 `*-stale-*`）；
/// 2. 构建一次带内嵌前端的调试二进制（等价 pm 的 pnpm build）。
/// 之后每个 worker 直接 spawn 该 exe（自起实例），不再需要 vite/devServer。
/// 构建输出实时继承到控制台（进度可见），成败也记录在 paim.log。
import fs from "node:fs";
import { execSync } from "node:child_process";
import { join } from "node:path";
import { e2eLog } from "./e2e-logger";
import { e2eTargetDir } from "./target-dir";

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
  const targetDir = e2eTargetDir();
  const started = Date.now();
  e2eLog.info(`[global-setup] 构建 e2e 二进制（target=${targetDir}）`);
  execSync("pnpm tauri build --debug --no-bundle", {
    cwd: join(import.meta.dirname, ".."),
    // 不吞输出：cargo/前端构建进度实时可见，避免看起来卡住
    stdio: "inherit",
    env: {
      ...process.env,
      CARGO_TARGET_DIR: targetDir,
      // auto：stderr 是 TTY 才显示进度，宽度自动探测；CI/重定向时自动隐藏
      CARGO_TERM_PROGRESS_WHEN: "auto",
    },
  });
  e2eLog.info(
    `[global-setup] e2e 二进制构建完成，用时 ${((Date.now() - started) / 1000).toFixed(1)}s`,
  );
}
