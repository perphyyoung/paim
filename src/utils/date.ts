// 时间通用工具

/** 将 SQLite 的 ISO 8601 UTC 时间串转为本地时间显示。 */
export function formatLocalTime(s: string | null): string {
  if (!s) return "";
  const d = new Date(s);
  return Number.isNaN(d.getTime()) ? s : d.toLocaleString();
}
