import { reactive } from "vue";

export interface ToastItem {
  id: number;
  message: string;
}

// 模块级单例状态，所有页面共享同一 toast 队列
const state = reactive<{ items: ToastItem[] }>({ items: [] });
let seq = 0;

export function useToast() {
  function showToast(message: string, duration = 2500) {
    const id = ++seq;
    state.items.push({ id, message });
    setTimeout(() => {
      const idx = state.items.findIndex((t) => t.id === id);
      if (idx >= 0) state.items.splice(idx, 1);
    }, duration);
  }

  return { toasts: state.items, showToast };
}
