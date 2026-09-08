/**
 * 标签筛选区 → 卡片 拖拽添加标签（图像/提示词主页共用）。
 *
 * 用 Pointer 事件模拟拖拽（WebView2 原生 HTML5 DnD 不触发 drop，
 * 同 TagManagerModal 的做法）。模块级单例状态：同一时刻只有一个拖拽。
 *
 * 流程：TagFilterPanel 普通标签 pointerdown → startTagDrag（移动超阈值才激活，
 * 不影响点击筛选）→ 全局 pointermove 用 elementFromPoint 找
 * [data-card-drop-id]，命中卡片直接切 .tag-drop-hover 类做高亮 →
 * pointerup 命中则回调页面注册的 onDrop。
 */
import { ref } from "vue";
import { commands } from "@/bindings";
import type { Ref } from "vue";
import type { ToastType } from "@/components/useToast";

/** 拖拽状态（TagFilterPanel 渲染浮动标签用） */
export const tagDrag = {
  /** 是否处于拖拽中（超阈值激活） */
  active: ref(false),
  /** 拖拽中的标签名（激活后有效） */
  tagName: ref(""),
  /** 浮动标签跟随光标位置 */
  x: ref(0),
  y: ref(0),
};

type CardTagDropFn = (cardId: string, tagName: string) => void;
let onDrop: CardTagDropFn | null = null;

/** 页面注册 drop 处理（返回取消注册函数） */
export function registerCardTagDrop(fn: CardTagDropFn): () => void {
  onDrop = fn;
  return () => {
    if (onDrop === fn) onDrop = null;
  };
}

const DRAG_THRESHOLD = 5;
let pendingName = "";
let origin: { x: number; y: number } | null = null;
let hoverEl: HTMLElement | null = null;

/** 指令式切换悬停卡片高亮（.tag-drop-hover，定义在 MediaCard 的 scoped style） */
function setHover(el: HTMLElement | null) {
  if (hoverEl === el) return;
  hoverEl?.classList.remove("tag-drop-hover");
  el?.classList.add("tag-drop-hover");
  hoverEl = el;
}

/** 筛选区普通标签 pointerdown 入口（特殊标签不调用） */
export function startTagDrag(e: PointerEvent, tagName: string) {
  if (e.button !== 0 || tagDrag.active.value) return;
  e.preventDefault(); // 阻止文本选择；click 不受影响（未跨元素移动时照常触发）
  pendingName = tagName;
  origin = { x: e.clientX, y: e.clientY };
  window.addEventListener("pointermove", onDragPointerMove);
  window.addEventListener("pointerup", onDragPointerUp);
  window.addEventListener("pointercancel", cancelDrag);
}

function onDragPointerMove(ev: PointerEvent) {
  if (!origin) return;
  if (!tagDrag.active.value) {
    if (Math.hypot(ev.clientX - origin.x, ev.clientY - origin.y) < DRAG_THRESHOLD) return;
    tagDrag.active.value = true;
    tagDrag.tagName.value = pendingName;
  }
  tagDrag.x.value = ev.clientX;
  tagDrag.y.value = ev.clientY;
  const el = document.elementFromPoint(ev.clientX, ev.clientY);
  setHover(el?.closest<HTMLElement>("[data-card-drop-id]") ?? null);
}

function onDragPointerUp() {
  const hitEl = hoverEl;
  const hitId = hitEl?.dataset.cardDropId ?? null;
  const { active, tagName } = tagDrag;
  const name = active.value ? tagName.value : "";
  cleanup();
  if (hitId && name) onDrop?.(hitId, name);
}

function cancelDrag() {
  cleanup();
}

function cleanup() {
  window.removeEventListener("pointermove", onDragPointerMove);
  window.removeEventListener("pointerup", onDragPointerUp);
  window.removeEventListener("pointercancel", cancelDrag);
  origin = null;
  pendingName = "";
  setHover(null);
  tagDrag.active.value = false;
  tagDrag.tagName.value = "";
}

export interface UseCardTagAddOptions {
  /** "image" | "prompt"，决定命令名，与 useTagAdd（详情弹窗）同一命令 */
  domain: "image" | "prompt";
  /** 卡片标签源（Record<id, 标签名[]>），成功后本地合并，即时反映到卡片 */
  tagNames: Ref<Record<string, string[]>>;
  /** 成功后刷新标签筛选区 */
  loadTagFilter: () => Promise<void> | void;
  showToast: (message: string, type?: ToastType) => void;
}

/**
 * 拖拽筛选区标签到卡片：快捷添加标签（图像/提示词主页共用）。
 * 走合一命令 addTag(domain, id, name)（单条目添加，非 batch），
 * 重复标签提示已存在，成功后本地合并 tagNames 并刷新筛选区。
 *
 * 两主页被 KeepAlive 缓存且共用单例回调注册：返回 activate/deactivate
 * 供页面在 onActivated/onDeactivated 中调用，保证 drop 回调始终属于当前页。
 */
export function useCardTagAdd(options: UseCardTagAddOptions) {
  const { domain, tagNames, loadTagFilter, showToast } = options;

  const dropFn = async (cardId: string, tagName: string) => {
    const existing = tagNames.value[cardId];
    if (existing?.includes(tagName)) {
      showToast(`标签「${tagName}」已存在`, "warning");
      return;
    }
    try {
      await commands.addTag(domain, cardId, tagName);
      tagNames.value = { ...tagNames.value, [cardId]: [...(existing ?? []), tagName] };
      showToast(`已添加标签「${tagName}」`, "success");
      await loadTagFilter();
    } catch (e) {
      showToast(`添加标签失败：${e}`, "error");
    }
  };

  let unregister: (() => void) | null = null;
  return {
    activate: () => {
      unregister?.();
      unregister = registerCardTagDrop(dropFn);
    },
    deactivate: () => {
      unregister?.();
      unregister = null;
    },
  };
}
