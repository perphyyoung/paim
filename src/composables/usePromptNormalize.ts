/**
 * 提示词「规范化」公共逻辑（提示词详情编辑态、提示词主页新建弹窗、图像详情新建提示词共用）。
 *
 * 只把调用方的草稿 ref 替换成后端 `canonical_text` 的**向量形态**（去首尾空白、
 * 删标点旁空白、其余连续空白折叠为一个空格，幂等），**不落库**：
 * 详情页点「取消」即回到库内原文，新建弹窗点「取消」即丢弃草稿，点「保存 / 确定」才写入。
 * 因此调用方只需保证传入的是编辑缓冲，展示态不要暴露入口。
 */
import type { Ref } from "vue";
import { commands } from "@/bindings";
import { useToast } from "@/components/useToast";

interface UsePromptNormalizeOptions {
  /**
   * 成功提示文案。详情页尚未落库，强调「保存后生效」；
   * 新建弹窗点「确定」才写入，用默认的「已按规范化格式填入」即可。
   */
  appliedMessage?: string;
}

export function usePromptNormalize(target: Ref<string>, options: UsePromptNormalizeOptions = {}) {
  const { showToast } = useToast();
  const { appliedMessage = "已按规范化格式填入" } = options;

  async function normalize() {
    const canon = await commands.canonicalizePromptText(target.value);
    // 空内容 / 纯空白：canonical 为空串，只提示不改值（避免把用户正在输入的空白清掉）
    if (!canon) {
      showToast("提示词内容为空", "warning");
      return;
    }
    if (canon === target.value) {
      showToast("已是规范化格式", "info");
      return;
    }
    target.value = canon;
    showToast(appliedMessage, "info");
  }

  return { normalize };
}
