/**
 * 图像悬停预览的定位数学（主页卡片与合并弹窗共用）。
 *
 * 输入锚点矩形与视口尺寸，输出浮层左上角、贴左侧还是右侧、以及可用最大宽度。
 * - placement `auto`：优先右侧，右侧放不下换左侧（主页虚拟网格用）。
 * - placement `left` / `right`：固定一侧（合并弹窗按源缩略图位置决定）。
 *
 * 只做几何计算，不碰 DOM、状态与样式；纵向按最大高度的一半夹在视口内。
 */

export type ImagePreviewPlacement = "left" | "right";

export interface ImagePreviewRect {
  left: number;
  right: number;
  top: number;
  height: number;
}

export interface ImagePreviewViewport {
  width: number;
  height: number;
}

export interface ImagePreviewPositionOptions {
  /** 期望贴哪一侧；默认 auto（自动换边） */
  placement?: "auto" | ImagePreviewPlacement;
  /** 与锚点之间的间隙（px） */
  gap?: number;
  /** 浮层宽度上限占视口比例 */
  maxWidthRatio?: number;
  /** 浮层高度上限占视口比例（用于纵向夹合） */
  maxHeightRatio?: number;
  /** 窄窗口下允许的最小宽度（px） */
  minWidth?: number;
}

export interface ImagePreviewPosition {
  left: number;
  top: number;
  placement: ImagePreviewPlacement;
  maxWidth: number;
}

export function computeImagePreviewPosition(
  rect: ImagePreviewRect,
  viewport: ImagePreviewViewport,
  options: ImagePreviewPositionOptions = {},
): ImagePreviewPosition {
  const gap = options.gap ?? 8;
  const maxWidthRatio = options.maxWidthRatio ?? 0.4;
  const maxHeightRatio = options.maxHeightRatio ?? 0.7;
  const minWidth = options.minWidth ?? 120;
  const vw = viewport.width;
  const vh = viewport.height;

  const rightSpace = Math.max(0, vw - rect.right - gap);
  const leftSpace = Math.max(0, rect.left - gap);
  const want = Math.min(vw * maxWidthRatio, 640);

  const placement: ImagePreviewPlacement =
    options.placement === "left" || options.placement === "right"
      ? options.placement
      : rightSpace >= want || rightSpace >= leftSpace
        ? "right"
        : "left";

  const space = placement === "right" ? rightSpace : leftSpace;
  const maxWidth = Math.max(minWidth, Math.min(vw * maxWidthRatio, space));

  // 浮层高度上限 maxHeightRatio：把中心线夹到「整块放得下」的区间里
  const halfH = Math.min(vh * maxHeightRatio, vh - 16) / 2;
  const minTop = halfH + gap;
  const maxTop = vh - halfH - gap;
  const center = rect.top + rect.height / 2;
  const top = maxTop >= minTop ? Math.min(maxTop, Math.max(minTop, center)) : vh / 2;

  return {
    left: placement === "right" ? rect.right + gap : rect.left - gap,
    top,
    placement,
    maxWidth,
  };
}
