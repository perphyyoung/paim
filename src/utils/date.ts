// 时间通用工具

/** 将 SQLite 的 UTC 时间串（"YYYY-MM-DD HH:MM:SS"）转为本地时间显示。 */
export function formatLocalTime(s: string | null): string {
  if (!s) return "";
  const d = new Date(s.replace(" ", "T") + "Z");
  return Number.isNaN(d.getTime()) ? s : d.toLocaleString();
}