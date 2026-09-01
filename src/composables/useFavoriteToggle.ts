/**
 * 收藏切换公共逻辑（图像/提示词主页共用）。
 *
 * 两个主页的单张/批量收藏流程同构：单张走 `update_{image|prompt}_detail`，
 * 批量走集合级切换 `batch_toggle_{image|prompt}_favorite`（对齐 pm 的 1-is_favorite 语义）。
 * 仅命令名与提示名词不同，经 domain 注入；列表变更由本模块写回调用方传入的 list。
 */
import type { Ref } from "vue";
import { invoke } from "@tauri-apps/api/core";

export interface FavoriteItem {
  id: string;
  is_favorite: boolean;
}

interface UseFavoriteToggleOptions<T extends FavoriteItem> {
  /** "image" | "prompt"，决定命令名与提示文案 */
  domain: "image" | "prompt";
  /** 切换后写回的当前列表 */
  list: Ref<T[]>;
  showToast: (message: string) => void;
}

export function useFavoriteToggle<T extends FavoriteItem>(options: UseFavoriteToggleOptions<T>) {
  const { domain, list, showToast } = options;
  const noun = domain === "image" ? "张图像" : "个提示词";
  const singleCmd = `update_${domain}_detail`;
  const batchCmd = `batch_toggle_${domain}_favorite`;

  // 单张切换：后端返回新对象，替换列表对应项
  async function toggleOne(item: T) {
    try {
      const updated = await invoke<T>(singleCmd, {
        id: item.id,
        isFavorite: !item.is_favorite,
      });
      list.value = list.value.map((x) => (x.id === updated.id ? updated : x));
    } catch (e) {
      showToast(`收藏失败：${e}`);
    }
  }

  // 批量切换：集合级翻转收藏状态，本地同步翻转；返回是否成功（成功后调用方再退出批量模式）
  async function toggleBatch(ids: string[]): Promise<boolean> {
    if (ids.length === 0) return false;
    try {
      const n = await invoke<number>(batchCmd, { ids });
      const sel = new Set(ids);
      list.value = list.value.map((x) =>
        sel.has(x.id) ? { ...x, is_favorite: !x.is_favorite } : x,
      );
      showToast(`已切换 ${n} ${noun}的收藏状态`);
      return true;
    } catch (e) {
      showToast(`批量切换收藏失败：${e}`);
      return false;
    }
  }

  return { toggleOne, toggleBatch };
}
