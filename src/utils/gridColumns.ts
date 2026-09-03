/**
 * 主页网格「显示列数」公共配置与状态（图像/提示词主页共用）。
 *
 * 列数是一等输入：卡片尺寸由容器宽与列数在 VirtualGrid 内推导（正方形卡片），
 * 这里只管范围限制与持久化，两页只传 domain 与默认值。
 */
import { ref } from "vue";

/** 显示列数范围限制，两页共用 */
export const GRID_COLUMNS_LIMITS = { min: 2, max: 12, step: 1 } as const;

/** 显示列数状态：localStorage 持久化，按域隔离（key 形如 image.columns / prompt.columns）。 */
export function useGridColumns(domain: string, initial: number) {
  const key = `${domain}.columns`;
  const columns = ref(Number(localStorage.getItem(key)) || initial);

  function setColumns(v: number) {
    const clamped = Math.min(
      GRID_COLUMNS_LIMITS.max,
      Math.max(GRID_COLUMNS_LIMITS.min, Math.round(v)),
    );
    columns.value = clamped;
    localStorage.setItem(key, String(clamped));
  }

  return { columns, setColumns };
}
