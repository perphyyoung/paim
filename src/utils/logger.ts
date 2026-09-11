import { commands, events } from "@/bindings";

// 前端日志：经 `log_msg` 命令写入 paim.log，与后端共用全局最低级别开关。
// - 启动时 getLogLevel 同步一次缓存；级别变更经 log-level-changed 事件刷新。
// - 本地预判级别，被过滤的日志零 IPC 开销（缓存就绪前先放行，避免丢 boot 日志）。
// - 发布版也可开 debug 排查（PAIM_LOG=debug 或 setLogLevel 热切）。

type Level = "debug" | "info" | "warn" | "error";

const LEVEL_NUM: Record<Level, number> = { debug: 0, info: 1, warn: 2, error: 3 };

// 未同步前放行全部（后端有级别过滤兜底）；同步后本地预判，被过滤的日志零 IPC
let minLevelNum = 0;
let synced = false;

commands
  .getLogLevel()
  .then((l) => {
    minLevelNum = LEVEL_NUM[l as Level] ?? 0;
    synced = true;
  })
  .catch(() => {});

events.logLevelChanged
  .listen((e) => {
    minLevelNum = LEVEL_NUM[e.payload as Level] ?? minLevelNum;
    synced = true;
  })
  .catch(() => {});

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

function send(level: Level, args: unknown[]): void {
  if (synced && LEVEL_NUM[level] < minLevelNum) return;
  commands.logMsg(level, fmt(args)).catch(() => {});
}

export const log = {
  debug: (...args: unknown[]) => send("debug", args),
  info: (...args: unknown[]) => send("info", args),
  warn: (...args: unknown[]) => send("warn", args),
  error: (...args: unknown[]) => send("error", args),
};

// 热切全局最低日志级别（后端与前端缓存同时生效）。
export async function setLogLevel(level: Level): Promise<void> {
  await commands.setLogLevel(level);
  // 兜底同步缓存（正常由 log-level-changed 事件驱动）
  minLevelNum = LEVEL_NUM[level];
  synced = true;
}
