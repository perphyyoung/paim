import { invoke } from "@tauri-apps/api/core";

// 前端调试日志：经 `log_msg` 命令写入根目录 / paim.log。
// 仅开发环境发送（避免发布后无谓 IPC 开销）。

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

function send(level: string, args: unknown[]): void {
  if (!import.meta.env.DEV) return;
  invoke("log_msg", { level, message: fmt(args) }).catch(() => {});
}

export const log = {
  debug: (...args: unknown[]) => send("debug", args),
  info: (...args: unknown[]) => send("info", args),
  warn: (...args: unknown[]) => send("warn", args),
  error: (...args: unknown[]) => send("error", args),
};