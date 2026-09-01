import { onActivated, onDeactivated, type Ref } from "vue";
import { useRouter } from "vue-router";

/**
 * 主页快捷键统一注册（提示词/图像对称）：
 * - Ctrl/Cmd+F 聚焦搜索
 * - F5 整页刷新（与左下「刷新缓存」同逻辑）
 * - Ctrl+P / Ctrl+I 切换到提示词 / 图像主页
 * - Ctrl+T 折叠/展开标签筛选区
 * - Ctrl+A 全选（焦点在输入框时放行原生文本全选）
 * KeepAlive 页面：激活期间监听、停用时移除（防止残留幽灵触发）。
 */
export function useHomeShortcuts(opts: {
  searchInput: Ref<HTMLInputElement | null>;
  tagFilter: Ref<{ toggleFilter: () => void } | null>;
  onSelectAll: () => void;
}) {
  const router = useRouter();

  function onSearchKeydown(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.code === "KeyF") {
      e.preventDefault();
      opts.searchInput.value?.focus();
    }
  }

  function onShortcutKeydown(e: KeyboardEvent) {
    if (e.key === "F5") {
      e.preventDefault();
      window.location.reload();
    } else if (e.ctrlKey || e.metaKey) {
      if (e.code === "KeyP") {
        e.preventDefault();
        router.push("/prompts");
      } else if (e.code === "KeyI") {
        e.preventDefault();
        router.push("/images");
      } else if (e.code === "KeyT") {
        e.preventDefault();
        opts.tagFilter.value?.toggleFilter();
      } else if (e.code === "KeyA") {
        const tag = (e.target as HTMLElement | null)?.tagName;
        if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT") return; // 输入框放行（文本全选）
        e.preventDefault();
        opts.onSelectAll();
      }
    }
  }

  onActivated(() => {
    document.addEventListener("keydown", onSearchKeydown);
    document.addEventListener("keydown", onShortcutKeydown);
  });
  onDeactivated(() => {
    document.removeEventListener("keydown", onSearchKeydown);
    document.removeEventListener("keydown", onShortcutKeydown);
  });
}
