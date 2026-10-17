// 详情「对侧快照」的整池作废 —— **收口点**：两类变化共用这里，两个详情页各只经一个本地入口调用它，
// 未来新增「改实体 / 改删除态」的入口也只调这里，避免再出现「本侧变了、对侧详情仍读旧快照」的漏失效
// （踩坑经过见 `lessons.md` 第 33 节，可复用的做法见 `开发经验.md` 第 8 节）。
//
// 两类变化都必须整池 clear 的原因：这两份快照的键是**对侧实体 id**（relatedImagesCache 键为 prompt id、
// relatedPromptsCache 键为 image id），受影响的对侧条目无法从本侧精确枚举 —— 只能整池作废：
//   ① 删除态：删除 / 恢复 / 彻底删除 / 清空回收站（后端按 is_deleted 过滤关联结果）
//   ② 内容·标签：标签增删 / 文件名·备注·标题·内容·翻译保存 / 替换图像（这些字段直接显示在对侧详情里）
// 关联关系本身的变化（关联 / 解除关联）粒度更细，仍用缓存的单条 `invalidate(id)`，不走这里。
//
// 为什么落在 views 层：分层规则要求 web-shared（components / composables / utils）不得依赖业务切片，
// 而这两个缓存分属 image / prompt 两个切片，只有视图层可以同时引用。

import { relatedPromptsCache } from "@/features/image/api/detailCache";
import { relatedImagesCache } from "@/features/prompt/api/relatedImagesCache";

/** 图像侧变化（① 删除态 ② 标签·文件名·备注·替换）→ 作废「提示词详情持有的图像快照」。
 *  快照含每张图的 file_name 与 tags，键是 prompt id，受影响提示词无法精确枚举 → 整池 clear。 */
export function invalidatePromptSideImageSnapshots(): void {
  relatedImagesCache.clear();
}

/** 提示词侧变化（① 删除态 ② 标题·内容·翻译·备注·标签）→ 作废「图像详情持有的提示词快照」。
 *  键是 image id，受影响图像无法精确枚举 → 整池 clear。 */
export function invalidateImageSidePromptSnapshots(): void {
  relatedPromptsCache.clear();
}
