/**
 * 图像/提示词条目布尔字段切换公共逻辑（收藏 / 安全，主页与详情弹窗共用）。
 *
 * 命令名按域拼 `update_{image|prompt}_detail`（单张/详情）/
 * `batch_toggle_{image|prompt}_favorite`（主页批量收藏，对齐 pm 的 1-is_favorite 语义）。
 * 仅命令名与提示名词不同，经 domain 注入；
 * 主页用 `toggleOne`/`toggleBatch`（列表写回），详情弹窗用 `toggleCurrent`（原地更新并通知父级）。
 */
import type { Ref } from "vue";
import { invoke } from "@tauri-apps/api/core";

export type BoolField = "is_favorite" | "is_safe";

interface BoolItem {
  id: string;
  is_favorite?: boolean;
  is_safe?: boolean;
}

/** 字段名（snake）→ invoke 参数名（camel） */
const FIELD_CAMEL: Record<BoolField, string> = {
  is_favorite: "isFavorite",
  is_safe: "isSafe",
};

interface UseItemToggleOptions<T extends BoolItem> {
  /** "image" | "prompt"，决定命令名与提示文案 */
  domain: "image" | "prompt";
  /** 主页单张/批量切换后写回的当前列表（详情弹窗可省略） */
  list?: Ref<T[]>;
  showToast: (message: string) => void;
}

export function useItemToggle<T extends BoolItem>(options: UseItemToggleOptions<T>) {
  const { domain, list, showToast } = options;
  const noun = domain === "image" ? "张图像" : "个提示词";
  const singleCmd = `update_${domain}_detail`;
  const batchCmd = `batch_toggle_${domain}_favorite`;

  // 详情弹窗单张切换：原地更新 current 对象并通知父级刷新
  async function toggleCurrent(current: Ref<T | null>, field: BoolField, emitChange: () => void) {
    const item = current.value;
    if (!item) return;
    try {
      const payload: Record<string, unknown> = {
        id: item.id,
        [FIELD_CAMEL[field]]: !item[field],
      };
      const upd = await invoke<T>(singleCmd, payload);
      item[field] = upd[field];
      emitChange();
    } catch {
      showToast("更新失败");
    }
  }

  // 主页单张切换：后端返回新对象，替换列表对应项
  async function toggleOne(item: T, field: BoolField) {
    if (!list) return;
    try {
      const payload: Record<string, unknown> = {
        id: item.id,
        [FIELD_CAMEL[field]]: !item[field],
      };
      const updated = await invoke<T>(singleCmd, payload);
      list.value = list.value.map((x) => (x.id === updated.id ? updated : x));
    } catch (e) {
      showToast(`更新失败：${e}`);
    }
  }

  // 主页批量切换收藏：集合级翻转收藏状态，本地同步翻转；返回是否成功（成功后调用方再退出批量模式）
  async function toggleBatch(ids: string[]): Promise<boolean> {
    if (ids.length === 0) return false;
    try {
      const n = await invoke<number>(batchCmd, { ids });
      const sel = new Set(ids);
      if (list) {
        list.value = list.value.map((x) =>
          sel.has(x.id) ? { ...x, is_favorite: !x.is_favorite } : x,
        );
      }
      showToast(`已切换 ${n} ${noun}的收藏状态`);
      return true;
    } catch (e) {
      showToast(`批量切换收藏失败：${e}`);
      return false;
    }
  }

  return { toggleCurrent, toggleOne, toggleBatch };
}
