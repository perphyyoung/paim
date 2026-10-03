// 提示词合并的词级对齐：提示词主体是逗号分隔的词组，字符级 diff 会在单词内部乱标，
// 故按「词 / 空白 / 标点」切 token 后做 LCS，公共 token 为相同部分，各自独有着红。
// token 切分保证 join(tokenize(s)) === s（无损），公共序列可直接拼成预填文本。

export type DiffKind = "common" | "removed" | "added";

export interface DiffToken {
  text: string;
  kind: DiffKind;
}

// 连续字母/数字（含中文等 Unicode 字词）| 连续空白 | 其它字符（标点逐个，逗号也单独成段）
const TOKEN_RE = /[\p{L}\p{N}]+|\s+|[^\p{L}\p{N}\s]/gu;

export function tokenize(text: string): string[] {
  return text.match(TOKEN_RE) ?? [];
}

/// LCS 长度表（标准 DP）。
function lcsTable(a: string[], b: string[]): number[][] {
  const dp: number[][] = Array.from({ length: a.length + 1 }, () =>
    new Array<number>(b.length + 1).fill(0),
  );
  for (let i = a.length - 1; i >= 0; i--) {
    for (let j = b.length - 1; j >= 0; j--) {
      dp[i][j] = a[i] === b[j] ? dp[i + 1][j + 1] + 1 : Math.max(dp[i + 1][j], dp[i][j + 1]);
    }
  }
  return dp;
}

/// 双侧 token 打平为一条带 kind 的序列（common 段两侧重合，removed/added 各自独有）。
export function diffTokens(a: string, b: string): DiffToken[] {
  const ta = tokenize(a);
  const tb = tokenize(b);
  const dp = lcsTable(ta, tb);
  const out: DiffToken[] = [];
  let i = 0;
  let j = 0;
  const push = (kind: DiffKind, text: string) => {
    const last = out[out.length - 1];
    // 同色相邻 token 合并，减少 span 数量；空白 token 也并入
    if (last && last.kind === kind) last.text += text;
    else out.push({ text, kind });
  };
  while (i < ta.length && j < tb.length) {
    if (ta[i] === tb[j]) {
      push("common", ta[i]);
      i++;
      j++;
    } else if (dp[i + 1][j] >= dp[i][j + 1]) {
      push("removed", ta[i]);
      i++;
    } else {
      push("added", tb[j]);
      j++;
    }
  }
  while (i < ta.length) push("removed", ta[i++]);
  while (j < tb.length) push("added", tb[j++]);
  return out;
}

/// 公共部分文本（保留原顺序与标点），作为合并内容的预填。
export function commonText(a: string, b: string): string {
  const ta = tokenize(a);
  const tb = tokenize(b);
  const dp = lcsTable(ta, tb);
  const parts: string[] = [];
  let i = 0;
  let j = 0;
  while (i < ta.length && j < tb.length) {
    if (ta[i] === tb[j]) {
      parts.push(ta[i]);
      i++;
      j++;
    } else if (dp[i + 1][j] >= dp[i][j + 1]) i++;
    else j++;
  }
  return parts.join("");
}
