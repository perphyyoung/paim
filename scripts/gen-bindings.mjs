// pnpm check 的 bindings 复写步骤：以「导出即退」模式启动调试主程序，重新生成
// src/bindings.ts（导出即退逻辑见 lib.rs run() 开头的 PAIM_EXPORT_BINDINGS 短路）。
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";

const root = path.join(import.meta.dirname, "..");
// 根目录 Cargo.toml 是 workspace 根，cargo 默认 target-dir 即 <项目根>/target；
// 兜底只在未设 CARGO_TARGET_DIR 时生效。与 e2e/e2e-helpers.ts 的 exePath 保持一致。
const targetDir = process.env.CARGO_TARGET_DIR ?? path.join(root, "target");
const exe = path.join(targetDir, "debug", "paim.exe");
const out = path.join(root, "src", "bindings.ts");

if (!fs.existsSync(exe)) {
  console.error(`未找到调试二进制 ${exe}（check 链路中的 cargo build 应先生成它）`);
  process.exit(1);
}

const r = spawnSync(exe, {
  env: { ...process.env, PAIM_EXPORT_BINDINGS: "1" },
  timeout: 10_000, // 正常毫秒级退出；超时说明二进制不含导出即退逻辑，属构建过期
});
if (r.error || r.status !== 0) {
  console.error(
    `导出即退进程失败：${r.error ?? `exit=${r.status}`}（请重新 cargo build 生成含 PAIM_EXPORT_BINDINGS 短路的调试二进制）`,
  );
  process.exit(1);
}
if (!fs.existsSync(out)) {
  console.error(`复写失败：${out} 不存在`);
  process.exit(1);
}
