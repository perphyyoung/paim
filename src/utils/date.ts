// 时间通用工具

/** 将 SQLite 的 ISO 8601 UTC 时间串转为本地时间显示。 */
export function formatLocalTime(s: string | null): string {
  if (!s) return "";
  const d = new Date(s);
  return Number.isNaN(d.getTime()) ? s : d.toLocaleString();
}

/**
 * 文件名用的本地时间戳（形如 `20260918-120000`）。
 * 完整备份与用户偏好导出共用同一格式，避免两处各写一套。
 */
export function fileTimestamp(): string {
  return new Date()
    .toLocaleString("sv-SE", { hour12: false })
    .replace(/[-: ]/g, (m) => (m === " " ? "-" : ""))
    .slice(0, 15);
}

/** 将时间串转为毫秒时间戳，无法解析返回 0。供排序比较用：
 * 库内时间可能混有 ISO 8601 与本地斜杠两种格式，字符串比较会因格式前缀错乱，
 * 统一转数值时间戳后再比较可跨格式正确排序。 */
export function toTimestamp(s: string): number {
  const t = new Date(s).getTime();
  return Number.isNaN(t) ? 0 : t;
}
