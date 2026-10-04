// 提示词关联图像的实体级缓存（与图像侧 `detailCache` 对称，共用 createEntityCache）。
// 失效时机：
// - 单条失效（invalidate）：关联关系变化（移除/设为首图/导入/替换），见 PromptDetailModal；
// - 整池失效（clear）：图像删除/恢复/彻底删除/清空回收站——后端按图像 is_deleted 过滤，
//   受影响提示词无法精确枚举，由 ImagePage 的生命周期入口统一 clear。
// 保存提示词字段不影响关联图像，不失效。

import { commands, type RelatedImage } from "@/bindings";
import { createEntityCache } from "@/utils/entityCache";

/** 提示词 id → 关联图像列表；单条较大，上限 100（约 1 MB 封顶） */
export const relatedImagesCache = createEntityCache<RelatedImage[]>(
  (id) => commands.getPromptRelatedImages(id),
  100,
);
