/**
 * 可搜索下拉的选项过滤：不区分大小写子串匹配，前缀命中优先，其次按名称排序。
 * 抽成纯函数便于单测；`SearchableSelect` 直接使用。
 */
export function filterOptions(options: string[], query: string): string[] {
  const q = query.trim().toLowerCase();
  if (!q) return options;
  const hit = options.filter((o) => o.toLowerCase().includes(q));
  return hit.sort((a, b) => {
    const ap = a.toLowerCase().startsWith(q) ? 0 : 1;
    const bp = b.toLowerCase().startsWith(q) ? 0 : 1;
    return ap - bp || a.localeCompare(b, undefined, { numeric: true });
  });
}
