/**
 * 全局字体设置。
 *
 * 字号大小：缩放比例持久化到 localStorage，并写入根元素 CSS 变量
 * `--font-size-scale`；样式层以 `--fs-*` token（calc(px * scale)）消费该变量，
 * 凡使用 token 的界面都随 `useFontScale` 统一缩放。后续字体相关能力（族/行高等）可扩展本文件。
 */
import { ref } from "vue";

/** 字体缩放比例范围（%）与步进 */
export const FONT_SCALE_LIMITS = { min: 75, max: 150, step: 5 } as const;

const FONT_SCALE_KEY = "fontScale";

/** 详情页正文字号缩放比例的存储键 */
const DETAIL_FONT_KEY = "detailFontScale";

/** 将缩放比例（%）写入根元素 CSS 变量 --font-size-scale */
export function applyFontScale(scale: number) {
  document.documentElement.style.setProperty("--font-size-scale", String(scale / 100));
}

/** 将详情页字号缩放比例（%）写入根元素 CSS 变量 --detail-font-scale */
export function applyDetailFontScale(scale: number) {
  document.documentElement.style.setProperty("--detail-font-scale", String(scale / 100));
}

function loadScale(key: string): number {
  const v = Number(localStorage.getItem(key));
  return Number.isFinite(v) && v > 0 ? v : 100;
}

/** 应用启动时调用一次，让未打开设置弹窗时缩放也生效 */
export function initFontScale() {
  applyFontScale(loadScale(FONT_SCALE_KEY));
  applyDetailFontScale(loadScale(DETAIL_FONT_KEY));
}

/** 全局字体大小状态：localStorage 持久化并即时应用 */
export function useFontScale() {
  const fontScale = ref(loadScale(FONT_SCALE_KEY));
  applyFontScale(fontScale.value);

  function setFontScale(v: number) {
    const clamped = Math.min(FONT_SCALE_LIMITS.max, Math.max(FONT_SCALE_LIMITS.min, v));
    fontScale.value = clamped;
    localStorage.setItem(FONT_SCALE_KEY, String(clamped));
    applyFontScale(clamped);
  }

  return { fontScale, setFontScale };
}

/** 详情页正文字号状态：localStorage 持久化并即时应用 */
export function useDetailFontScale() {
  const detailFontScale = ref(loadScale(DETAIL_FONT_KEY));
  applyDetailFontScale(detailFontScale.value);

  function setDetailFontScale(v: number) {
    const clamped = Math.min(FONT_SCALE_LIMITS.max, Math.max(FONT_SCALE_LIMITS.min, v));
    detailFontScale.value = clamped;
    localStorage.setItem(DETAIL_FONT_KEY, String(clamped));
    applyDetailFontScale(clamped);
  }

  return { detailFontScale, setDetailFontScale };
}
