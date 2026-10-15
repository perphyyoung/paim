/**
 * 导航胶囊「手动输入序号」的取值解析（纯函数，便于单测）。
 *
 * 入参 `totalCount` 是总条数；返回 **1-based 序号**（导航胶囊 `1 / N` 的口径），
 * 非法输入返回 `null`（调用方据此回退当前序号，不跳）；合法值夹到 `[1, totalCount]`
 * —— 输入 999 → 末项、0 与负数 → 首项，与「直接跳到末项」的直觉一致。
 *
 * 需要 0-based 数组下标时，由调用方在**唯一一处**减 1（见 `ImageFullscreenViewer.jumpTo1basedIndex`）。
 */
export function parse1basedIndexInput(raw: string, totalCount: number): number | null {
  if (totalCount <= 0) return null;
  const trimmed = raw.trim();
  if (!/^[+-]?\d+$/.test(trimmed)) return null;
  const n = Number(trimmed);
  if (!Number.isFinite(n)) return null;
  return Math.min(totalCount, Math.max(1, n));
}
