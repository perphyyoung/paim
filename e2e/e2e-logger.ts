/// e2e 测试侧日志：封装与前端 `src/utils/logger.ts` 一致的调用方式
/// （log.debug/info/warn/error，多参空格拼接、对象自动 JSON 序列化），
/// 用例打日志的写法与前端打点完全相同。
/// 与前端 logger 的区别仅在实现环境：e2e 跑在 Playwright worker（Node）里，
/// 没有 Tauri IPC，因此内部用 fs 追加写 <项目根>/paim.log（与应用日志同文件，
/// 行带 [E2E w<n>] 前缀区分测试侧与业务侧），写失败静默，不阻塞用例。
/// 命名与前端 logger.ts 区分开（e2e-logger.ts / e2eLog），避免误以为走 IPC。
import fs from "node:fs";
import path from "node:path";

/// worker 标识，由 launchApp 按 workerIndex 设置（每个 worker 是独立进程，模块态安全）
let workerTag = "";

export function setWorkerTag(workerIndex: number): void {
  workerTag = ` w${workerIndex}`;
}

/// 与前端 logger 的 fmt 一致：多参空格拼接，非字符串（含对象）JSON 序列化
function fmt(args: unknown[]): string {
  return args
    .map((a) => {
      if (typeof a === "string") return a;
      try {
        return JSON.stringify(a);
      } catch {
        return String(a);
      }
    })
    .join(" ");
}

function timestamp(): string {
  const d = new Date();
  const p = (n: number, len = 2) => String(n).padStart(len, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(
    d.getMinutes(),
  )}:${p(d.getSeconds())}.${p(d.getMilliseconds(), 3)}`;
}

function send(level: "DEBUG" | "INFO" | "WARN" | "ERROR", args: unknown[]): void {
  try {
    const tag = workerTag ? ` ${workerTag.trim()}` : "";
    fs.appendFileSync(
      path.join(import.meta.dirname, "..", "paim.log"),
      `${timestamp()} [${level}] [E2E${tag}] ${fmt(args)}\n`,
    );
  } catch {
    // 写失败静默
  }
}

export const e2eLog = {
  debug: (...args: unknown[]) => send("DEBUG", args),
  info: (...args: unknown[]) => send("INFO", args),
  warn: (...args: unknown[]) => send("WARN", args),
  error: (...args: unknown[]) => send("ERROR", args),
};
