// 时间通用工具

/** 将 SQLite 的 ISO 8601 UTC 时间串转为本地时间显示。 */
export function formatLocalTime(s: string | null): string {
  if (!s) return "";
  const d = new Date(s);
  return Number.isNaN(d.getTime()) ? s : d.toLocaleString();
}

/**
 * 文件名用的本地时间戳（形如 `20260918-120000`）。
 * 前端只此一处产出（完整备份、用户偏好导出的默认文件名用它）；
 * 后端同名实现是 `infra/time.rs::file_stamp()`（让位备份目录、孤儿文件导出目录用它）——
 * 跨语言无法共享代码，**改格式必须两端同步**。
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
