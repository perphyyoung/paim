/**
 * 卡片尺寸公共配置与状态（图像/提示词主页共用）。
 *
 * 参考 pm：PanelManagerBase 统一 cardSize 状态与应用逻辑，子类仅提供 storage key；
 * 这里把范围限制与持久化逻辑收敛到一处，两页只传 domain 与默认值。
 */
import { ref } from "vue";

/** 卡片边长范围限制（px），两页共用一个滑块范围 */
export const CARD_SIZE_LIMITS = { min: 160, max: 400, step: 20 } as const;

/**
 * 卡片边长状态：localStorage 持久化，按域隔离（key 形如 image.cardSize / prompt.cardSize）。
 * 返回的 cardSize 直接用于虚拟滚动项宽高与卡片标签行布局。
 */
export function useCardSize(domain: string, initial: number) {
  const key = `${domain}.cardSize`;
  const cardSize = ref(Number(localStorage.getItem(key)) || initial);

  function setCardSize(v: number) {
    cardSize.value = v;
    localStorage.setItem(key, String(v));
  }

  function onSizeInput(e: Event) {
    setCardSize(Number((e.target as HTMLInputElement).value));
  }

  return { cardSize, setCardSize, onSizeInput };
}
