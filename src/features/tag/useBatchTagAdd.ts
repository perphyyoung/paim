/**
 * 批量添加标签公共逻辑（图像/提示词主页共用）。
 *
 * 两个主页的批量打标签流程同构：命令名按域拼 `add_{image|prompt}_tag_batch`，
 * 成功后提示、退出批量模式并刷新标签数据。仅提示名词不同，经 domain 注入。
 */
import type { Ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { isSpecialTag } from "./specialTags";

export interface UseBatchTagAddOptions {
  /** "image" | "prompt"，决定命令名与提示文案 */
  domain: "image" | "prompt";
  selectedIds: Ref<Set<string>>;
  /** 成功后退出批量模式 */
  exitBatch: () => void;
  /** 成功后刷新标签筛选区（图像侧负责刷新卡片标签源） */
  loadTagFilter: () => Promise<void> | void;
  showToast: (message: string) => void;
}

export function useBatchTagAdd(options: UseBatchTagAddOptions) {
  const { domain, selectedIds, exitBatch, loadTagFilter, showToast } = options;
  const command = `add_${domain}_tag_batch`;
  const noun = domain === "image" ? "张图像" : "个提示词";

  async function batchAddTag(tag: string) {
    const ids = Array.from(selectedIds.value);
    if (ids.length === 0) return;
    const name = tag.trim();
    if (isSpecialTag(name)) {
      showToast(`「${name}」是系统特殊标签，不能手动添加`);
      return;
    }
    try {
      await invoke(command, { ids, name });
      showToast(`已为 ${ids.length} ${noun}添加标签`);
      exitBatch();
      await loadTagFilter();
    } catch (e) {
      showToast(`批量添加标签失败：${e}`);
    }
  }

  return { batchAddTag };
}
