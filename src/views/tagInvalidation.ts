// 标签管理保存后的统一失效：本侧详情标签缓存 + 对侧详情快照 + 自动完成候选。
//
// 收口原因：标签改名 / 合并 / 删除会同时影响「本侧详情里的标签」与「对侧详情里的标签名」，
// 两侧数据来源不同（图像详情 imageTagsCache / 提示词详情 promptTagsCache），
// 分散在页面里写容易漏（曾漏过 imageTagsCache）。新增标签写入口时只调这一个函数。
import { imageTagsCache } from "@/features/image/api/detailCache";
import { promptTagsCache } from "@/features/prompt/api/tagCache";
import { invalidateTagCandidates } from "@/features/tag/useTagCandidates";
import {
  invalidateImageSidePromptSnapshots,
  invalidatePromptSideImageSnapshots,
} from "./detailEntityInvalidation";

/** `domain` 是「发起变更的标签域」，清的是本侧详情标签缓存 + 对侧详情快照。 */
export function invalidateAfterTagManager(domain: "image" | "prompt"): void {
  invalidateTagCandidates();
  if (domain === "image") {
    imageTagsCache.clear();
    invalidatePromptSideImageSnapshots();
  } else {
    promptTagsCache.clear();
    invalidateImageSidePromptSnapshots();
  }
}
