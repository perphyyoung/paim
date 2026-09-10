// 提示词卡片背景的缩略图自愈：与图像侧对称的适配层。
//
// 背景取自「关联首图」，因此校验的入参是**提示词 id**（后端自行解析到关联图像），
// 返回的是背景路径发生变化的提示词，供页面只刷新这几张卡片。
import { commands } from "@/bindings";
import type { ThumbnailEnsureFixed, ThumbnailEnsureResult } from "@/bindings";

/** 懒自愈单条修复结果（id 此处为提示词 id） */
export type { ThumbnailEnsureFixed };

/** 批量校验可见提示词的卡片背景，缺失且原图存在时按需生成并回写 */
export async function ensurePromptThumbnails(ids: string[]): Promise<ThumbnailEnsureResult> {
  if (ids.length === 0) return { fixed: [], missing: [] };
  return { fixed: await commands.ensurePromptThumbnails(ids), missing: [] };
}
