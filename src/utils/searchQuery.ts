/**
 * 主页搜索的词法解析：正则开关 + `/.../` 包裹语法。
 *
 * 只有两种模式：默认子串（后端 LIKE），正则（后端 REGEXP，走 `infra::regexp`）。
 * `/sole/` 视为正则包裹（与编辑器 / 浏览器一致），开关关闭也能用；空输入永远不过滤。
 * 注意：正则引擎是 Rust regex，**不支持 lookaround**（`(?=)` / `(?<=)`），整词请写 `\bsole\b`
 * （这样 `camisole` / `console` 不会被命中）。
 */
export type SearchMode = "Substring" | "Regex";

export interface ParsedSearch {
  search: string;
  mode: SearchMode;
}

export function parseSearch(raw: string, regexMode: boolean): ParsedSearch {
  const text = raw.trim();
  if (!text) return { search: "", mode: "Substring" };
  // 首尾都是 `/` 且中间非空：去掉包裹斜杠，按正则处理
  if (text.length > 2 && text.startsWith("/") && text.endsWith("/")) {
    return { search: text.slice(1, -1), mode: "Regex" };
  }
  return { search: text, mode: regexMode ? "Regex" : "Substring" };
}
