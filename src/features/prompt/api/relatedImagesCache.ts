// 提示词关联图像的实体级缓存（与图像侧 `detailCache` 对称，共用 createEntityCache）。
// 失效时机：
// - 单条失效（invalidate）：关联关系变化（移除/设为首图/导入/替换），见 PromptDetailModal；
// - 整池失效（clear）：图像删除/恢复/彻底删除/清空回收站——后端按图像 is_deleted 过滤，
//   受影响提示词无法精确枚举，由 ImagePage 的生命周期入口统一 clear；
//   另一条整池失效：**图像内容/标签变化**（图像详情的标签增删、文件名/备注保存、替换图像）——
//   这些字段直接显示在本缓存的「关联图像」项里（含每张图的 tags），键按 prompt id 无法精确枚举，
//   由 ImagePage.onDetailChanged（详情"内容已变"信号）经 views/detailEntityInvalidation 统一 clear（收口点）。
// 保存提示词字段不影响关联图像，不失效。

import { commands, type RelatedImage } from "@/bindings";
import { createEntityCache } from "@/utils/entityCache";

/** 提示词 id → 关联图像列表；单条较大，上限 100（约 1 MB 封顶） */
export const relatedImagesCache = createEntityCache<RelatedImage[]>(
  (id) => commands.getPromptRelatedImages(id),
  100,
);
