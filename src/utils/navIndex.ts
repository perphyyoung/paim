/**
 * 导航胶囊「手动输入序号」的取值解析（纯函数，便于单测）。
 *
 * 规则：去掉首尾空白后必须能解析成整数，否则返回 `null`（调用方据此回退到当前序号，不跳）；
 * 合法值夹到 `[1, orderLength]`：输入 999 → 末项、0 与负数 → 首项，与「直接跳到末项」的直觉一致。
 * 返回的是 1-based 序号（界面显示的 `1 / N` 口径），调用方再转成 0-based。
 */
export function resolveIndexInput(raw: string, orderLength: number): number | null {
  if (orderLength <= 0) return null;
  const trimmed = raw.trim();
  if (!/^[+-]?\d+$/.test(trimmed)) return null;
  const n = Number(trimmed);
  if (!Number.isFinite(n)) return null;
  return Math.min(orderLength, Math.max(1, n));
}
