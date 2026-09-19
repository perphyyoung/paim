// 图像详情的实体级缓存（与提示词侧 `relatedImagesCache` 对称，共用 createEntityCache），
// 外加原图 URL 的两个取值入口（peek 同步 / resolve 异步）。
// 失效只在数据本身变化时做：提示词关联变化、替换图像、标签增删。

import { commands, type LinkedPrompt, type TagLite } from "@/bindings";
import { toAssetUrl } from "@/utils/assetUrl";
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

/**
 * 命中缓存即同步返回原图 asset URL（未命中返回 null）：
 * 详情页据此先渲染已缓存的图，只对未命中的 id 发请求，避免切图时闪一下 loading。
 */
export function peekImageSrc(id: string): string | null {
  const cached = imageSrcCache.get(id);
  return cached ? toAssetUrl(cached) : null;
}

/** 按 id 取原图 asset URL：发命令（同 id 并发去重）后回填缓存；失败原样抛出，由调用方决定兜底 */
export async function resolveImageSrc(id: string): Promise<string> {
  return toAssetUrl(await imageSrcCache.fetch(id));
}
