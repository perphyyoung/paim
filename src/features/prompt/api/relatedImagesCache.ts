// 提示词关联图像的实体级缓存（与图像侧 `detailCache` 对称，共用 createEntityCache）。
// 失效只在「关联关系变化」的操作上做（移除/设为首图/导入/替换），保存提示词字段不影响。

import { commands, type RelatedImage } from "@/bindings";
import { createEntityCache } from "@/utils/entityCache";

/** 提示词 id → 关联图像列表 */
export const relatedImagesCache = createEntityCache<RelatedImage[]>((id) =>
  commands.getPromptRelatedImages(id),
);
