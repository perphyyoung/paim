// 通用确认管理：配合 ConfirmDialog.vue 使用。
// 调用 ask(message, options?, onConfirm) 弹出确认，确认后执行 onConfirm。
import { ref } from "vue";

export interface ConfirmOptions {
  title?: string;
  confirmText?: string;
  danger?: boolean;
}

export function useConfirm() {
  const confirmOpen = ref(false);
  const confirmTitle = ref("确认");
  const confirmMessage = ref("");
  const confirmText = ref("确定");
  const confirmDanger = ref(false);
  let callback: (() => void | Promise<void>) | null = null;

  /** 弹出确认框；onConfirm 可为异步。 */
  function ask(message: string, options?: ConfirmOptions, onConfirm?: () => void | Promise<void>) {
    confirmMessage.value = message;
    if (options?.title) confirmTitle.value = options.title;
    else confirmTitle.value = "确认";
    if (options?.confirmText) confirmText.value = options.confirmText;
    else confirmText.value = "确定";
    confirmDanger.value = options?.danger ?? false;
    callback = onConfirm ?? null;
    confirmOpen.value = true;
  }

  function cancelConfirm() {
    confirmOpen.value = false;
    callback = null;
  }

  async function confirmAction() {
    confirmOpen.value = false;
    const cb = callback;
    callback = null;
    if (cb) await cb();
  }

  return {
    confirmOpen,
    confirmTitle,
    confirmMessage,
    confirmText,
    confirmDanger,
    ask,
    cancelConfirm,
    confirmAction,
  };
}