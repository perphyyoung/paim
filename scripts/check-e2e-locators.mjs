// e2e 定位器卫生检查：Playwright 的位置参数（getByText / getByTitle / getByLabel / getByAltText /
// getByPlaceholder）与 getByRole 的 `name` 默认都是**子串匹配**，会互相吞并：
// 例如 `提示词内容` ⊂ `以提示词内容搜索相似项`（aria-label），`收藏` ⊂ `取消收藏`，`关闭` ⊂ `关闭查找`。
// 子串匹配既会误红，也会误绿（断言 A 却命中 B），因此规则是：名称匹配默认 `exact: true`。
//
// 用法：
//   node scripts/check-e2e-locators.mjs         # 报告：title / label / placeholder / alt 全仓强制，
//                                               #       text / role-name 只查两个详情弹窗作用域
//   node scripts/check-e2e-locators.mjs --fix   # 自动补 exact: true
//   node scripts/check-e2e-locators.mjs --all   # 连 text / role-name 的页面级调用一起查（仍有历史欠账）
// 确需子串匹配的调用，在**调用所在行**加注释 `locator-allow-substring` 豁免。
import { readFileSync, readdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const DIR = fileURLToPath(new URL("../e2e/", import.meta.url));
const NAME_METHODS = new Set(["Text", "Title", "Label", "AltText", "Placeholder"]);
const ALLOW = "locator-allow-substring";
/// 默认只检查「两个详情弹窗」作用域的调用：这些定位器共用一套按钮名，最容易互相吞并。
/// 页面级调用（page.getByText(...) 等）暂不强制，--all 可整体查看。
const DETAIL_RECEIVERS = new Set([
  "detail",
  "bottom",
  "pd",
  "promptDetail",
  "imageDetail",
  "nestedPrompt",
  "promptSlot",
  "imageSlot",
  "imgDetail",
]);
/// 这几个 API 的参数是「稀缺语义名」（title / label / placeholder / alt），全仓一律要求 exact：
/// 它们几乎总能精确表达，且最容易与动态文案（文件名、`文件名：…` tooltip）撞车。
const ALWAYS_EXACT_KINDS = new Set(["Title", "Label", "AltText", "Placeholder"]);

/// 从 `(` 位置找到配对的 `)`，跳过字符串、模板串、行/块注释与嵌套括号。
function matchParen(src, open) {
  let depth = 0;
  for (let i = open; i < src.length; i++) {
    const c = src[i];
    if (c === "'" || c === '"' || c === "`") {
      const quote = c;
      for (i++; i < src.length; i++) {
        if (src[i] === "\\") i++;
        else if (src[i] === quote) break;
      }
      continue;
    }
    if (c === "/" && src[i + 1] === "/") {
      const nl = src.indexOf("\n", i);
      i = nl < 0 ? src.length : nl;
      continue;
    }
    if (c === "/" && src[i + 1] === "*") {
      const end = src.indexOf("*/", i + 2);
      i = end < 0 ? src.length : end + 1;
      continue;
    }
    if (c === "(") depth++;
    else if (c === ")" && --depth === 0) return i;
  }
  return -1;
}

/// 按顶层逗号切参数（忽略字符串与嵌套括号内的逗号）。
function splitArgs(body) {
  const args = [];
  let depth = 0;
  let cur = "";
  for (let i = 0; i < body.length; i++) {
    const c = body[i];
    if (c === "'" || c === '"' || c === "`") {
      cur += c;
      for (i++; i < body.length; i++) {
        cur += body[i];
        if (body[i] === "\\") cur += body[++i] ?? "";
        else if (body[i] === c) break;
      }
      continue;
    }
    if ("([{".includes(c)) depth++;
    if (")]}".includes(c)) depth--;
    if (c === "," && depth === 0) {
      args.push(cur.trim());
      cur = "";
      continue;
    }
    cur += c;
  }
  if (cur.trim()) args.push(cur.trim());
  return args;
}

/// 判断一次调用是否需要（且能够）补 exact；返回改动后的调用体，或 null 表示无需改/不可改。
function fixBody(kind, body) {
  if (/\bexact\s*:/.test(body)) return null;
  const args = splitArgs(body);
  if (NAME_METHODS.has(kind)) {
    const first = args[0] ?? "";
    if (!first || first.startsWith("/")) return null; // RegExp 字面量：保持正则
    if (args.length > 1 && args[1].startsWith("{")) {
      const obj = args[1];
      return [first, obj.replace(/^\{\s*/, "{ exact: true, "), ...args.slice(2)].join(", ");
    }
    return [first, "{ exact: true }", ...args.slice(1)].join(", ");
  }
  if (kind === "Role") {
    if (args.length < 2) return null; // 没有 options（如 getByRole("img")）
    const opts = args[1];
    if (!opts.startsWith("{") || !/\bname\s*:/.test(opts)) return null;
    return [args[0], opts.replace(/^\{\s*/, "{ exact: true, "), ...args.slice(2)].join(", ");
  }
  return null;
}

function lineOf(src, index) {
  return src.slice(0, index).split("\n").length;
}

function scan(src, file) {
  const found = [];
  const re = /getBy(Text|Title|Label|AltText|Placeholder|Role)\s*\(/g;
  let m;
  while ((m = re.exec(src))) {
    const open = m.index + m[0].length - 1;
    const close = matchParen(src, open);
    if (close < 0) continue;
    // 跳过注释里的示例（行注释 / 块注释行首 *）
    const lineStart = src.lastIndexOf("\n", m.index) + 1;
    const prefix = src.slice(lineStart, m.index);
    if (prefix.includes("//") || /^\s*\*/.test(prefix)) {
      re.lastIndex = close;
      continue;
    }
    const receiver = /([A-Za-z_$][\w$]*)\s*\.\s*$/.exec(src.slice(Math.max(0, m.index - 60), m.index))?.[1];
    const inScope =
      process.argv.includes("--all") ||
      ALWAYS_EXACT_KINDS.has(m[1]) ||
      DETAIL_RECEIVERS.has(receiver ?? "");
    if (!inScope) {
      re.lastIndex = close;
      continue;
    }
    const body = src.slice(open + 1, close);
    const lineText = src.slice(lineStart, src.indexOf("\n", close) < 0 ? src.length : src.indexOf("\n", close));
    const hasAllow = lineText.includes(ALLOW) || prefix.includes(ALLOW);
    const fixed = fixBody(m[1], body);
    if (fixed !== null && !hasAllow) {
      found.push({ kind: m[1], open, close, body, fixed, file, line: lineOf(src, m.index) });
    }
    re.lastIndex = close;
  }
  return found;
}

const violations = [];
const files = readdirSync(DIR).filter((f) => f.endsWith(".ts"));

for (const file of files) {
  const path = join(DIR, file);
  const src = readFileSync(path, "utf8");
  const found = scan(src, file);
  if (found.length === 0) continue;
  violations.push(...found);
  if (!process.argv.includes("--fix")) continue;
  let out = src;
  for (const v of [...found].reverse()) {
    out = out.slice(0, v.open + 1) + v.fixed + out.slice(v.close);
  }
  writeFileSync(path, out);
}

if (violations.length === 0) {
  console.log("e2e 定位器检查通过：所有名称匹配都已精确（或已豁免）");
  process.exit(0);
}

for (const v of violations) {
  console.log(`${v.file}:${v.line}  getBy${v.kind}  ${process.argv.includes("--fix") ? "已补 exact" : "缺少 exact"}`);
}
const total = violations.length;
if (process.argv.includes("--fix")) {
  console.log(`\n共修正 ${total} 处`);
  process.exit(0);
}
console.log(`\n共 ${total} 处名称匹配未精确。修复：node scripts/check-e2e-locators.mjs --fix`);
console.log(`确需子串匹配的调用，在该行加注释 ${ALLOW} 豁免。`);
process.exit(1);
