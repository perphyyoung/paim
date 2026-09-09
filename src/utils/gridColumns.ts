/**
 * 主页网格「显示列数」公共配置与状态（图像/提示词主页共用）。
 *
 * 列数是一等输入：卡片尺寸由容器宽与列数在 VirtualGrid 内推导（正方形卡片），
 * 这里只管范围限制与持久化，两页只传 domain 与默认值。
 */
import { ref } from "vue";

/** 显示列数范围限制，两页共用 */
export const GRID_COLUMNS_LIMITS = { min: 2, max: 12 } as const;

/** 固定尺寸网格的卡片边长（= 缩略图尺寸，如回收站），不随容器缩放 */
export const FIXED_CARD_SIZE = 200;

/**
 * 卡片大小滑杆的档位换算：档位与列数**反向**——向右拖 = 档位增大 = 列数减少 = 卡片变大
 * （滑杆只换交互方向，底层仍以列数为一等输入，持久化的还是列数）。
 * 档位 0 = 最小卡片（列数取 max），档位 max−min = 最大卡片（列数取 min）。
 */
export function columnsToSizeLevel(columns: number): number {
  return GRID_COLUMNS_LIMITS.max - columns;
}

export function sizeLevelToColumns(level: number): number {
  return GRID_COLUMNS_LIMITS.max - level;
}

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
