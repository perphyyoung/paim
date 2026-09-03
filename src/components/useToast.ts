import { reactive } from "vue";

export type ToastType = "info" | "success" | "error" | "warning";

export interface ToastItem {
  id: number;
  message: string;
  type: ToastType;
}

// 模块级单例状态，所有页面共享同一 toast 队列
const state = reactive<{ items: ToastItem[] }>({ items: [] });
let seq = 0;

/** 各类型默认停留时长：全部居中显示，错误/警告停留更久确保被注意到 */
const DEFAULT_DURATION: Record<ToastType, number> = {
  info: 2500,
  success: 2500,
  error: 4000,
  warning: 4000,
};

export function useToast() {
  function showToast(message: string, type: ToastType = "info", duration?: number) {
    const id = ++seq;
    state.items.push({ id, message, type });
    setTimeout(() => {
      dismissToast(id);
    }, duration ?? DEFAULT_DURATION[type]);
  }

  /** 手动移除一条 toast（点击 toast 提前关闭时使用） */
  function dismissToast(id: number) {
    const idx = state.items.findIndex((t) => t.id === id);
    if (idx >= 0) state.items.splice(idx, 1);
  }

  return { toasts: state.items, showToast, dismissToast };
}
