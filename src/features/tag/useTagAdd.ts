/**
 * 详情弹窗「添加标签」公共逻辑（图像/提示词详情共用）。
 *
 * 收口两处几乎重复的实现：输入框一次只添加一个标签（不再支持逗号/空格批量），
 * 命令名经 options 注入，命令返回新增的标签列表并合并到本地 tags 快照。
 */
import { ref, type Ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { isSpecialTag } from "./specialTags";

export interface TagLite {
  id: number;
  name: string;
}

export interface UseTagAddOptions {
  /** 后端命令，如 "add_image_tag" / "add_prompt_tag"，接收 { id, name } */
  command: string;
  /** 返回当前详情项的 id（详情关闭后可能为 undefined） */
  getItemId: () => string | number | undefined;
  /** 本地标签快照，添加成功后合并 */
  tags: Ref<TagLite[]>;
  /** 用户提示（透传 app 的 showToast） */
  showToast: (message: string) => void;
  /** 添加成功后的额外回调（如广播数据变更事件） */
  onAdded?: (count: number) => void;
}

export function useTagAdd(options: UseTagAddOptions) {
  const { command, getItemId, tags, showToast, onAdded } = options;
  const tagInput = ref("");

  /** 一次只添加一个标签；返回本次新增数量（0 表示未添加） */
  async function addTag(): Promise<number> {
    const id = getItemId();
    const name = tagInput.value.trim();
    if (!id) return 0;
    if (!name) return 0;
    if (isSpecialTag(name)) {
      showToast(`「${name}」是系统特殊标签，不能手动添加`);
      return 0;
    }
    try {
      const added = await invoke<TagLite[]>(command, { id, name });
      tagInput.value = "";
      for (const t of added) {
        if (!tags.value.some((x) => x.id === t.id)) tags.value.push(t);
      }
      showToast(`已添加 ${added.length} 个标签`);
      onAdded?.(added.length);
      return added.length;
    } catch (e) {
      showToast(`添加标签失败：${e}`);
      return 0;
    }
  }

  return { tagInput, addTag };
}
