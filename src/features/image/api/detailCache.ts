// 图像详情的实体级缓存（与提示词侧 `relatedImagesCache` 对称，共用 createEntityCache）。
// 失效只在数据本身变化时做：提示词关联变化、替换图像、标签增删。

import { commands, type LinkedPrompt, type TagLite } from "@/bindings";
import { createEntityCache } from "@/utils/entityCache";

/** 图像 id → 关联提示词（含标题/内容/翻译/备注/标签）；单条较大，上限 100（约 1 MB 封顶） */
export const relatedPromptsCache = createEntityCache<LinkedPrompt[]>(
  (id) => commands.getImageRelatedPrompts(id),
  100,
);

/** 图像 id → 原图绝对路径（详情页展示原图，不同于卡片缩略图）；路径稳定，可长期缓存 */
export const imageSrcCache = createEntityCache<string>((id) => commands.getImageSrc(id), 300);

/** 图像 id → 图像标签 */
export const imageTagsCache = createEntityCache<TagLite[]>(
  (id) => commands.getItemTags("image", id),
  300,
);
