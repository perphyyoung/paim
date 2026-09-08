// 图像详情的实体级缓存（与提示词侧 `relatedImagesCache` 对称，共用 createEntityCache）。
// 失效只在数据本身变化时做：提示词关联变化、替换图像、标签增删。

import { commands, type ImageTag, type LinkedPrompt } from "@/bindings";
import { createEntityCache } from "@/utils/entityCache";

/** 图像 id → 关联提示词（含标题/内容/翻译/备注/标签） */
export const relatedPromptsCache = createEntityCache<LinkedPrompt[]>((id) =>
  commands.getImageRelatedPrompts(id),
);

/** 图像 id → 原图绝对路径（详情页展示原图，不同于卡片缩略图）；路径稳定，可长期缓存 */
export const imageSrcCache = createEntityCache<string>((id) => commands.getImageSrc(id));

/** 图像 id → 图像标签 */
export const imageTagsCache = createEntityCache<ImageTag[]>((id) => commands.getImageTags(id));
