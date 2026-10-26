/**
 * 主页卡片「悬停原图预览」的全局单例状态机（仅主页用；合并弹窗走 MergePromptImagePreview）。
 *
 * 需求：鼠标停在卡片上**满 1 秒**才浮出原图大图，扫过不弹（整行卡片划过时不该满屏闪图）。
 * 用单例而不是每张卡一份状态，原因有三：
 * 1. 主页是虚拟网格，卡片会被滚动复用/重建——每卡一份计时器会出现「卡滚走了浮层还在」的错位；
 * 2. 同一时刻只该有一张预览，后停留者覆盖前者；
 * 3. 原图路径缓存、滚动取消、请求竞态（seq）只做一份。
 *
 * 交互口径（调用方：MediaCard 的 mouseenter/mouseleave；关闭监听由 HomeCardHoverPreviewLayer 绑定）：
 * - `scheduleHomeCardHoverPreview` 只起计时器，**到点才请求原图**——鼠标扫过不发 IPC；
 * - `closeHomeCardHoverPreview` 清计时器 + 作废在途请求 + 隐藏浮层；
 * - 浮层 `pointer-events-none`，不拦截卡片上的任何点击/拖拽。
 */
import { ref } from "vue";
import { computeImagePreviewPosition } from "@/utils/imagePreviewPosition";
import { peekImageSrc, resolveImageSrc } from "@/utils/imageSrcCache";
import { readLocalStorageValue, writeLocalStorageValue } from "@/utils/localStorage";

/** 悬停多久才弹原图：默认值（设置面板未改过时用它；测试与文档同口径） */
export const HOME_CARD_HOVER_PREVIEW_DELAY_MS = 1_000;

/** 延迟可配置范围与步进（设置面板滑块与钳制逻辑同口径，改范围只改这一处） */
export const HOME_CARD_HOVER_PREVIEW_DELAY_LIMITS = { min: 200, max: 3_000, step: 100 } as const;

/** localStorage 键：延迟（毫秒）与总开关（"0" 关闭；缺省开启） */
const DELAY_KEY = "homeCardHoverPreviewDelay";
const ENABLED_KEY = "homeCardHoverPreviewEnabled";

/** 把任意输入收敛到合法延迟：非数字回退默认值，越界钳制到范围端点 */
export function clampHomeCardHoverPreviewDelay(ms: number): number {
  if (!Number.isFinite(ms)) return HOME_CARD_HOVER_PREVIEW_DELAY_MS;
  return Math.min(
    HOME_CARD_HOVER_PREVIEW_DELAY_LIMITS.max,
    Math.max(HOME_CARD_HOVER_PREVIEW_DELAY_LIMITS.min, Math.round(ms)),
  );
}

function loadDelay(): number {
  const raw = readLocalStorageValue(DELAY_KEY);
  if (raw === null || raw.trim() === "") return HOME_CARD_HOVER_PREVIEW_DELAY_MS;
  return clampHomeCardHoverPreviewDelay(Number(raw));
}

/** 全局配置：设置面板改这里，已挂载的卡片无需重建即可生效 */
export const homeCardHoverPreviewDelayMs = ref(loadDelay());
export const homeCardHoverPreviewEnabled = ref(readLocalStorageValue(ENABLED_KEY) !== "0");

/** 改延迟：写入偏好并收起当前浮层（下次悬停按新值计时） */
export function setHomeCardHoverPreviewDelay(ms: number): void {
  homeCardHoverPreviewDelayMs.value = clampHomeCardHoverPreviewDelay(ms);
  writeLocalStorageValue(DELAY_KEY, String(homeCardHoverPreviewDelayMs.value));
  closeHomeCardHoverPreview();
}

/** 总开关：关闭立即收起浮层并停止后续排期 */
export function setHomeCardHoverPreviewEnabled(enabled: boolean): void {
  homeCardHoverPreviewEnabled.value = enabled;
  writeLocalStorageValue(ENABLED_KEY, enabled ? "1" : "0");
  if (!enabled) closeHomeCardHoverPreview();
}

export interface HomeCardHoverPreviewAnchor {
  /** 原图对应的图像 id（提示词卡片传「关联首图」的 id，与卡片背景同一张） */
  id: string;
  /** 锚点元素：卡片根节点，用来算浮层位置 */
  el: HTMLElement;
}

export const homeCardHoverPreviewVisible = ref(false);
export const homeCardHoverPreviewUrl = ref("");
export const homeCardHoverPreviewLoading = ref(false);
export const homeCardHoverPreviewFailed = ref(false);
/** 浮层左上角（placement=left 时配合 translateX(-100%) 右边缘对齐） */
export const homeCardHoverPreviewPos = ref({ left: 0, top: 0 });
export const homeCardHoverPreviewPlacement = ref<"right" | "left">("right");
/** 浮层最大宽度（按锚点两侧实际可用空间算，避免溢出视口） */
export const homeCardHoverPreviewMaxWidth = ref(480);

// 计时器句柄用 ReturnType 而不是 number：DOM lib 返回 number、@types/node 返回 Timeout，
// 显式依赖前者会在不同类型包加载顺序/TS 版本下报 2322
let timer: ReturnType<typeof setTimeout> | null = null;
let anchor: HomeCardHoverPreviewAnchor | null = null;
/** 请求序号：close / 换锚点 / 新请求都会作废在途响应 */
let seq = 0;

/**
 * 悬停进入：起 1s 计时器（同一锚点重复调用只刷新元素引用，不重置计时）。
 * delay 可调（主页固定 1000ms；单测用它缩短等待）。
 */
export function scheduleHomeCardHoverPreview(
  next: HomeCardHoverPreviewAnchor,
  delay = homeCardHoverPreviewDelayMs.value,
): void {
  if (!homeCardHoverPreviewEnabled.value) return;
  if (anchor?.id === next.id && (timer !== null || homeCardHoverPreviewVisible.value)) {
    anchor = next; // 卡片 DOM 可能被复用重排，位置信息以最新节点为准
    return;
  }
  closeHomeCardHoverPreview();
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
export function closeHomeCardHoverPreview(): void {
  if (timer !== null) {
    clearTimeout(timer);
    timer = null;
  }
  if (!anchor && !homeCardHoverPreviewVisible.value) return;
  anchor = null;
  seq++;
  homeCardHoverPreviewVisible.value = false;
  // 保留 URL：浮层用 v-show 常驻，同一张图二次悬停可直接复用已解码位图；
  // 换图时 openPreview 会先把 url 置空，不会串图
  homeCardHoverPreviewLoading.value = false;
  homeCardHoverPreviewFailed.value = false;
}

/** 到点后打开：**再核验一次**锚点（等待期间可能已移出 / 卡片已被虚拟滚动复用） */
async function openPreview(target: HomeCardHoverPreviewAnchor): Promise<void> {
  if (anchor !== target || !target.el.isConnected) return;
  const pos = computeHomeCardHoverPreviewPosition(target.el);
  homeCardHoverPreviewPos.value = { left: pos.left, top: pos.top };
  homeCardHoverPreviewPlacement.value = pos.placement;
  homeCardHoverPreviewMaxWidth.value = pos.maxWidth;
  homeCardHoverPreviewVisible.value = true;
  homeCardHoverPreviewFailed.value = false;

  const cached = peekImageSrc(target.id);
  if (cached) {
    seq++;
    homeCardHoverPreviewUrl.value = cached;
    homeCardHoverPreviewLoading.value = false;
    return;
  }

  const my = ++seq;
  homeCardHoverPreviewUrl.value = "";
  homeCardHoverPreviewLoading.value = true;
  try {
    const resolved = await resolveImageSrc(target.id);
    if (my === seq) homeCardHoverPreviewUrl.value = resolved;
  } catch {
    if (my === seq) homeCardHoverPreviewFailed.value = true;
  } finally {
    if (my === seq) homeCardHoverPreviewLoading.value = false;
  }
}

/** 按锚点元素算浮层位置（auto 换边；几何细节见 utils/imagePreviewPosition） */
export function computeHomeCardHoverPreviewPosition(
  el: HTMLElement,
): ReturnType<typeof computeImagePreviewPosition> {
  return computeImagePreviewPosition(
    el.getBoundingClientRect(),
    { width: window.innerWidth, height: window.innerHeight },
    { placement: "auto" },
  );
}
