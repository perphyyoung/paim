// 图像详情的实体级缓存（与提示词侧 `relatedImagesCache` 对称，共用 createEntityCache），
// 外加原图 URL 的两个取值入口（peek 同步 / resolve 异步）。
// 失效只在数据本身变化时做：提示词关联变化、替换图像、标签增删；
// relatedPromptsCache 额外在提示词删除/恢复/彻底删除/清空回收站时整池 clear
// （后端按提示词 is_deleted 过滤，入口在 PromptPage）；
// 另一条整池 clear：**提示词内容/标签变化**（提示词详情的保存、标签增删）——这些字段直接显示在
// 本缓存的「关联提示词」项里，键按 image id 无法精确枚举，由 PromptPage.onDetailChanged 经
// views/detailEntityInvalidation 统一 clear（收口点）。

import { commands, type LinkedPrompt, type TagLite } from "@/bindings";
import { createEntityCache } from "@/utils/entityCache";

/** 图像 id → 关联提示词（含标题/内容/翻译/备注/标签）；单条较大，上限 100（约 1 MB 封顶） */
export const relatedPromptsCache = createEntityCache<LinkedPrompt[]>(
  (id) => commands.getImageRelatedPrompts(id),
  100,
);

/** 图像 id → 原图绝对路径（详情页展示原图，不同于卡片缩略图）；路径稳定，可长期缓存 */
// 原图路径缓存已下沉到 utils（详情 / 全屏 / 悬停预览共用同一份），
// 这里 re-export 保持既有 import 不变。
export {
  clearImageSrcCache,
  invalidateImageSrc,
  peekImageSrc,
  resolveImageSrc,
} from "@/utils/imageSrcCache";

/** 图像 id → 图像标签 */
export const imageTagsCache = createEntityCache<TagLite[]>(
  (id) => commands.getItemTags("image", id),
  300,
);
