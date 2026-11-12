import path from "node:path";

const ROOT = path.join(import.meta.dirname, "..");

/// e2e 产物与 check/dev 的共享 target 完全隔离：两者 feature 集不同，
/// 同目录会互相覆盖无哈希产物并触发重编。
/// 放在共享 target 的 `e2e` 子目录：所有项目共用，custom-protocol 依赖只编一份。
export function e2eTargetDir(): string {
  const shared = process.env.CARGO_TARGET_DIR;
  return shared ? path.join(shared, "e2e") : path.join(ROOT, "target", "e2e");
}

export function appExePath(): string {
  return path.join(e2eTargetDir(), "debug", "paim.exe");
}
