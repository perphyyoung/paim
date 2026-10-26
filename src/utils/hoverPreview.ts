/**
 * 主页卡片「悬停原图预览」的全局单例状态机。
 *
 * 需求：鼠标停在卡片上**满 1 秒**才浮出原图大图，扫过不弹（整行卡片划过时不该满屏闪图）。
 * 用单例而不是每张卡一份状态，原因有三：
 * 1. 主页是虚拟网格，卡片会被滚动复用/重建——每卡一份计时器会出现「卡滚走了浮层还在」的错位；
 * 2. 同一时刻只该有一张预览，后停留者覆盖前者；
 * 3. 原图 URL 缓存、滚动取消、请求竞态（seq）只做一份。
 *
 * 交互口径（调用方：MediaCard 的 mouseenter/mouseleave；关闭监听由 HoverPreviewLayer 统一绑定）：
 * - `scheduleHoverPreview` 只起计时器，**到点才请求原图**——鼠标扫过不发 IPC；
 * - `closeHoverPreview` 清计时器 + 作废在途请求 + 隐藏浮层；移出卡片、点击、右键、滚动、拖拽、resize 都调它；
 * - 浮层 `pointer-events-none`，不拦截卡片上的任何点击/拖拽。
 */
import { ref } from "vue";
import { peekImageSrc, resolveImageSrc } from "@/utils/imageSrcCache";
import { readLocalStorageValue, writeLocalStorageValue } from "@/utils/localStorage";

/** 悬停多久才弹原图：默认值（设置面板未改过时用它；测试与文档同口径） */
export const HOVER_PREVIEW_DELAY_MS = 1_000;

/** 延迟可配置范围与步进（设置面板滑块与钳制逻辑同口径，改范围只改这一处） */
export const HOVER_PREVIEW_DELAY_LIMITS = { min: 200, max: 3_000, step: 100 } as const;

/** localStorage 键：延迟（毫秒）与总开关（"0" 关闭；缺省开启） */
const DELAY_KEY = "homeCardHoverPreviewDelay";
const ENABLED_KEY = "homeCardHoverPreviewEnabled";

/** 把任意输入收敛到合法延迟：非数字回退默认值，越界钳制到范围端点 */
export function clampHoverPreviewDelay(ms: number): number {
  if (!Number.isFinite(ms)) return HOVER_PREVIEW_DELAY_MS;
  return Math.min(
    HOVER_PREVIEW_DELAY_LIMITS.max,
    Math.max(HOVER_PREVIEW_DELAY_LIMITS.min, Math.round(ms)),
  );
}

function loadDelay(): number {
  const raw = readLocalStorageValue(DELAY_KEY);
  if (raw === null || raw.trim() === "") return HOVER_PREVIEW_DELAY_MS;
  return clampHoverPreviewDelay(Number(raw));
}

/** 全局配置：设置面板改这里，已挂载的卡片无需重建即可生效 */
export const hoverPreviewDelayMs = ref(loadDelay());
export const hoverPreviewEnabled = ref(readLocalStorageValue(ENABLED_KEY) !== "0");

/** 改延迟：写入偏好并收起当前浮层（下次悬停按新值计时） */
export function setHoverPreviewDelay(ms: number): void {
  hoverPreviewDelayMs.value = clampHoverPreviewDelay(ms);
  writeLocalStorageValue(DELAY_KEY, String(hoverPreviewDelayMs.value));
  closeHoverPreview();
}

/** 总开关：关闭立即收起浮层并停止后续排期 */
export function setHoverPreviewEnabled(enabled: boolean): void {
  hoverPreviewEnabled.value = enabled;
  writeLocalStorageValue(ENABLED_KEY, enabled ? "1" : "0");
  if (!enabled) closeHoverPreview();
}

/** 浮层与锚点卡片之间的间隙（px） */
const GAP = 8;
/** 浮层宽度上限：视口的 40%，与图片 max-w 口径一致 */
const MAX_WIDTH_RATIO = 0.4;
/** 浮层宽度下限（窄窗口两侧都挤时至少给这么多，宁可贴边也不挤成一条线） */
const MIN_WIDTH = 120;

export interface HoverPreviewAnchor {
  /** 原图对应的图像 id（提示词卡片传「关联首图」的 id，与卡片背景同一张） */
  id: string;
  /** 锚点元素：卡片根节点，用来算浮层位置 */
  el: HTMLElement;
}

export const hoverPreviewVisible = ref(false);
export const hoverPreviewUrl = ref("");
export const hoverPreviewLoading = ref(false);
export const hoverPreviewFailed = ref(false);
/** 浮层左上角（placement=left 时配合 translateX(-100%) 右边缘对齐） */
export const hoverPreviewPos = ref({ left: 0, top: 0 });
export const hoverPreviewPlacement = ref<"right" | "left">("right");
/** 浮层最大宽度（按锚点两侧实际可用空间算，避免溢出视口） */
export const hoverPreviewMaxWidth = ref(480);

/** image id → 原图 asset URL */
// 原图路径缓存统一走 utils/imageSrcCache（与详情/全屏共用、LRU 封顶），不再单独维护 Map

// 计时器句柄用 ReturnType 而不是 number：DOM lib 返回 number、@types/node 返回 Timeout，
// 显式依赖前者会在不同类型包加载顺序/TS 版本下报 2322
let timer: ReturnType<typeof setTimeout> | null = null;
let anchor: HoverPreviewAnchor | null = null;
/** 请求序号：close / 换锚点 / 新请求都会作废在途响应 */
let seq = 0;

/**
 * 悬停进入：起 1s 计时器（同一锚点重复调用只刷新元素引用，不重置计时）。
 * delay 可调（主页固定 1000ms；单测用它缩短等待）。
 */
export function scheduleHoverPreview(
  next: HoverPreviewAnchor,
  delay = hoverPreviewDelayMs.value,
): void {
  if (!hoverPreviewEnabled.value) return;
  if (anchor?.id === next.id && (timer !== null || hoverPreviewVisible.value)) {
    anchor = next; // 卡片 DOM 可能被复用重排，位置信息以最新节点为准
    return;
  }
  closeHoverPreview();
  anchor = next;
  if (delay <= 0) {
    void openPreview(next);
    return;
  }
  timer = setTimeout(() => {
    timer = null;
    void openPreview(next);
  }, delay);
}

/** 关闭（移出卡片 / 点击 / 右键 / 滚动 / 拖拽 / resize）：清计时器并按需作废在途请求 */
export function closeHoverPreview(): void {
  if (timer !== null) {
    clearTimeout(timer);
    timer = null;
  }
  if (!anchor && !hoverPreviewVisible.value) return;
  anchor = null;
  seq++;
  hoverPreviewVisible.value = false;
  // 保留 hoverPreviewUrl：浮层用 v-show 常驻，同一张图二次悬停可直接复用已解码位图；
  // 换图时 openPreview 会先把 url 置空，不会串图
  hoverPreviewLoading.value = false;
  hoverPreviewFailed.value = false;
}

/** 到点后打开：**再核验一次**锚点（等待期间可能已移出 / 卡片已被虚拟滚动复用） */
async function openPreview(target: HoverPreviewAnchor): Promise<void> {
  if (anchor !== target || !target.el.isConnected) return;
  const pos = computePosition(target.el);
  hoverPreviewPos.value = { left: pos.left, top: pos.top };
  hoverPreviewPlacement.value = pos.placement;
  hoverPreviewMaxWidth.value = pos.maxWidth;
  hoverPreviewVisible.value = true;
  hoverPreviewFailed.value = false;

  const cached = peekImageSrc(target.id);
  if (cached) {
    seq++;
    hoverPreviewUrl.value = cached;
    hoverPreviewLoading.value = false;
    return;
  }

  const my = ++seq;
  hoverPreviewUrl.value = "";
  hoverPreviewLoading.value = true;
  try {
    const resolved = await resolveImageSrc(target.id);
    if (my === seq) hoverPreviewUrl.value = resolved;
  } catch {
    if (my === seq) hoverPreviewFailed.value = true;
  } finally {
    if (my === seq) hoverPreviewLoading.value = false;
  }
}

/**
 * 浮层位置：优先卡片右侧，右侧放不下换左侧；两侧都窄时取空间大的一侧并压缩宽度。
 * 纵向以卡片中线为基准上下居中，夹在视口内（与浮层 70vh 上限同一口径）。
 */
export function computePosition(el: HTMLElement): {
  left: number;
  top: number;
  placement: "right" | "left";
  maxWidth: number;
} {
  const rect = el.getBoundingClientRect();
  const vw = window.innerWidth;
  const vh = window.innerHeight;
  const rightSpace = Math.max(0, vw - rect.right - GAP);
  const leftSpace = Math.max(0, rect.left - GAP);
  const want = Math.min(vw * MAX_WIDTH_RATIO, 640);

  const placement: "right" | "left" =
    rightSpace >= want || rightSpace >= leftSpace ? "right" : "left";
  const space = placement === "right" ? rightSpace : leftSpace;
  const maxWidth = Math.max(MIN_WIDTH, Math.min(vw * MAX_WIDTH_RATIO, space));

  // 浮层最大高度 70vh（与模板 max-h-[70vh] 一致）：把中心线夹到「整块放得下」的区间里
  const halfH = Math.min(vh * 0.7, vh - 16) / 2;
  const minTop = halfH + GAP;
  const maxTop = vh - halfH - GAP;
  const center = rect.top + rect.height / 2;
  const top = maxTop >= minTop ? Math.min(maxTop, Math.max(minTop, center)) : vh / 2;

  return {
    left: placement === "right" ? rect.right + GAP : rect.left - GAP,
    top,
    placement,
    maxWidth,
  };
}
