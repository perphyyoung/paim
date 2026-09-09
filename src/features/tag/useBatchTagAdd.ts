/**
 * 批量添加标签公共逻辑（图像/提示词主页共用）。
 *
 * 两个主页的批量打标签流程同构：统一走合一的 batchAddTag(domain, ids, name)，
 * 成功后提示、退出批量模式并刷新标签数据。仅提示名词不同，经 domain 注入。
 */
import type { Ref } from "vue";
import { commands } from "@/bindings";
import { isSpecialTag } from "./specialTags";
import { addTagName } from "./useTagCandidates";
import type { ToastType } from "@/components/useToast";

export interface UseBatchTagAddOptions {
  /** "image" | "prompt"，决定命令名与提示文案 */
  domain: "image" | "prompt";
  selectedIds: Ref<Set<string>>;
  /** 条目 id → 已有标签名（用于预检「已存在」，避免后端空刷新 updated_at） */
  tagNames: Ref<Record<string, string[]>>;
  /** 成功后退出批量模式 */
  exitBatch: () => void;
  /** 成功后刷新标签筛选区（图像侧负责刷新卡片标签源） */
  loadTagFilter: () => Promise<void> | void;
  showToast: (message: string, type?: ToastType) => void;
}

export function useBatchTagAdd(options: UseBatchTagAddOptions) {
  const { domain, selectedIds, tagNames, exitBatch, loadTagFilter, showToast } = options;
  const noun = domain === "image" ? "张图像" : "个提示词";

  /**
   * 返回是否成功（成功后调用方再关闭批量添加标签弹窗）。
   * 已存在的条目不参与提交：全部已存在时视为未成功，弹窗保持打开便于改名。
   */
  async function batchAddTag(tag: string): Promise<boolean> {
    const ids = Array.from(selectedIds.value);
    if (ids.length === 0) return false;
    const name = tag.trim();
    if (isSpecialTag(name)) {
      showToast(`「${name}」是系统特殊标签，不能手动添加`, "warning");
      return false;
    }
    // 预检拆分：present 已有该标签，missing 需要真正提交
    const present: string[] = [];
    const missing: string[] = [];
    for (const id of ids) {
      const owned = tagNames.value[id];
      if (owned?.includes(name)) present.push(id);
      else missing.push(id);
    }
    if (missing.length === 0) {
      showToast(`选中的 ${ids.length} ${noun}已存在该标签`, "warning");
      return false;
    }
    try {
      await commands.batchAddTag(domain, missing, name);
      // 候选即时补名
      addTagName(name);
      if (present.length > 0) {
        showToast(`已为 ${missing.length} ${noun}添加标签，${present.length} 个已存在`, "warning");
      } else {
        showToast(`已为 ${missing.length} ${noun}添加标签`, "success");
      }
      exitBatch();
      await loadTagFilter();
      return true;
    } catch (e) {
      showToast(`批量添加标签失败：${e}`, "error");
      return false;
    }
  }

  return { batchAddTag };
}
