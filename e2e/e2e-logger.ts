/// e2e 测试侧日志：封装与前端 `src/utils/logger.ts` 一致的调用方式
/// （log.debug/info/warn/error，多参空格拼接、对象自动 JSON 序列化），
/// 用例打日志的写法与前端打点完全相同。
/// 与前端 logger 的区别仅在实现环境：e2e 跑在 Playwright worker（Node）里，
/// 没有 Tauri IPC，因此内部用 fs 追加写 <项目根>/paim.log（与应用日志同文件，
/// 行带 [E2E w<n>] 前缀区分测试侧与业务侧），写失败静默，不阻塞用例。
/// 命名与前端 logger.ts 区分开（e2e-logger.ts / e2eLog），避免误以为走 IPC。
import fs from "node:fs";
import path from "node:path";

/// 实例标识，由 launchApp 按 workerIndex + 本 worker 内第几个实例设置
/// （每个 worker 是独立进程，模块态安全）。同一 worker 顺序跑多个 spec 文件，
/// 每文件一个实例，故标识形如 w0-1（worker 0 的第 2 个文件实例）。
let workerTag = "";

export function setWorkerTag(tag: string): void {
  workerTag = ` ${tag}`;
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

/// 输出级别阈值：低于阈值的日志不写入。经 playwright.config.ts 设置
/// （PAIM_E2E_LOG_LEVEL，默认 warn），跑全量用例只记异常信号，
/// 排查失败时在配置里临时改为 info/debug 后重跑即可看到步骤细节。
const LEVELS = { DEBUG: 0, INFO: 1, WARN: 2, ERROR: 3 } as const;
type Level = keyof typeof LEVELS;
const threshold =
  LEVELS[(process.env.PAIM_E2E_LOG_LEVEL as Level | undefined)?.toUpperCase() as Level] ??
  LEVELS.WARN;

function write(text: string): void {
  try {
    fs.appendFileSync(path.join(import.meta.dirname, "..", "paim.log"), `${timestamp()} ${text}\n`);
  } catch {
    // 写失败静默
  }
}

function send(level: Level, args: unknown[]): void {
  if (LEVELS[level] < threshold) return;
  const tag = workerTag ? ` ${workerTag.trim()}` : "";
  write(`[${level}] [E2E${tag}] ${fmt(args)}`);
}

/// 用例分节标记（形如 `[TEST] [E2E w0] ▶ 06-toast-notification › 点击 toast …`）。
/// 与业务日志共用阈值（INFO）——默认 warn 下不写（跑全量只看异常信号），
/// 排查失败时把 PAIM_E2E_LOG_LEVEL 调到 info，即可在日志里看到每个用例的开始/结果分节。
/// worker 号由调用方传入：用例标题记录发生在应用实例启动之前，
/// 那时 launchApp 还没设置 workerTag（实例序号此刻也不存在；同 worker 的多个文件由文件名区分）。
export function testLog(workerIndex: number, ...args: unknown[]): void {
  if (LEVELS.INFO < threshold) return;
  write(`[TEST] [E2E w${workerIndex}] ${fmt(args)}`);
}

export const e2eLog = {
  debug: (...args: unknown[]) => send("DEBUG", args),
  info: (...args: unknown[]) => send("INFO", args),
  warn: (...args: unknown[]) => send("WARN", args),
  error: (...args: unknown[]) => send("ERROR", args),
};
