/**
 * 详情弹窗「添加标签」公共逻辑（图像/提示词详情共用）。
 *
 * 收口两处几乎重复的实现：输入框一次只添加一个标签（不再支持逗号/空格批量），
 * 命令名经 options 注入，命令返回新增的标签列表并合并到本地 tags 快照。
 */
import { computed, ref, type Ref } from "vue";
import type { TagLite } from "@/bindings";
import { isSpecialTag } from "./specialTags";
import { addTagName } from "./useTagCandidates";
import type { ToastType } from "@/components/useToast";

export interface UseTagAddOptions {
  /** 添加标签的命令函数（bindings 的 addTag + domain），接收 (id, name) */
  addTagCommand: (id: string, name: string) => Promise<TagLite[]>;
  /** 返回当前详情项的 id（详情关闭后可能为 undefined） */
  getItemId: () => string | number | undefined;
  /** 本地标签快照，添加成功后合并 */
  tags: Ref<TagLite[]>;
  /** 用户提示（透传 app 的 showToast） */
  showToast: (message: string, type?: ToastType) => void;
  /** 添加成功后的额外回调（如广播数据变更事件） */
  onAdded?: (count: number) => void;
}

export function useTagAdd(options: UseTagAddOptions) {
  const { addTagCommand, getItemId, tags, showToast, onAdded } = options;
  const tagInput = ref("");

  /** 当前条目已有标签名集合：随 tags 变化重建一次，之后判定为成员判断（不再逐个比对） */
  const ownedNames = computed(() => new Set(tags.value.map((t) => t.name)));

  /** 一次只添加一个标签；返回本次新增数量（0 表示未添加） */
  async function addTag(): Promise<number> {
    const id = getItemId();
    const name = tagInput.value.trim();
    if (!id) return 0;
    if (!name) return 0;
    if (isSpecialTag(name)) {
      showToast(`「${name}」是系统特殊标签，不能手动添加`, "warning");
      return 0;
    }
    // 已存在前置拦截：命中则提示并保持输入（不发命令，避免后端空刷新 updated_at）
    if (ownedNames.value.has(name)) {
      showToast(`标签「${name}」已存在`, "warning");
      return 0;
    }
    try {
      const added = await addTagCommand(String(id), name);
      tagInput.value = "";
      // 单次只添加一个标签，且提交前已确认不在 tags 中，直接合并无需回查
      const addedTag = added[0];
      if (addedTag) tags.value.push(addedTag);
      // 候选即时补名：下次输入无需重新拉取即可提示新标签
      addTagName(name);
      showToast(`已添加标签「${name}」`, "success");
      onAdded?.(added.length);
      return added.length;
    } catch (e) {
      showToast(`添加标签失败：${e}`, "error");
      return 0;
    }
  }

  return { tagInput, addTag };
}
