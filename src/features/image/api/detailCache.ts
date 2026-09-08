// 图像详情的实体级缓存（与提示词侧 `relatedImagesCache` 对称）。
// 详情弹窗被父级 v-if 强制卸载，组件内 ref 存不住，故挂在模块作用域：
// 切换/重开同一张图时不再重复读库。
// 失效只在数据本身变化时做：提示词关联变化、替换图像、标签增删。

import { commands, type ImageTag, type LinkedPrompt } from "@/bindings";

const promptsCache = new Map<string, LinkedPrompt[]>();
const srcCache = new Map<string, string>();
const tagsCache = new Map<string, ImageTag[]>();
// 并发去重：同一 key 的多个调用共用一个 in-flight Promise，只发一次命令
const inflight = new Map<string, Promise<unknown>>();

function dedupe<T>(key: string, run: () => Promise<T>): Promise<T> {
  const pending = inflight.get(key) as Promise<T> | undefined;
  if (pending) return pending;
  const task = run().finally(() => inflight.delete(key));
  inflight.set(key, task);
  return task;
}

/** 关联提示词（含标题/内容/翻译/备注/标签） */
export function getCachedRelatedPrompts(imageId: string): LinkedPrompt[] | undefined {
  return promptsCache.get(imageId);
}

export function fetchRelatedPrompts(imageId: string): Promise<LinkedPrompt[]> {
  return dedupe(`prompts:${imageId}`, async () => {
    const list = await commands.getImageRelatedPrompts(imageId);
    promptsCache.set(imageId, list);
    return list;
  });
}

export function invalidateRelatedPrompts(imageId: string): void {
  promptsCache.delete(imageId);
}

/** 原图绝对路径（详情页展示原图，不同于卡片缩略图）；路径稳定，可长期缓存 */
export function getCachedImageSrc(imageId: string): string | undefined {
  return srcCache.get(imageId);
}

export function fetchImageSrc(imageId: string): Promise<string> {
  return dedupe(`src:${imageId}`, async () => {
    const p = await commands.getImageSrc(imageId);
    srcCache.set(imageId, p);
    return p;
  });
}

export function invalidateImageSrc(imageId: string): void {
  srcCache.delete(imageId);
}

/** 图像标签 */
export function getCachedImageTags(imageId: string): ImageTag[] | undefined {
  return tagsCache.get(imageId);
}

export function fetchImageTags(imageId: string): Promise<ImageTag[]> {
  return dedupe(`tags:${imageId}`, async () => {
    const list = await commands.getImageTags(imageId);
    tagsCache.set(imageId, list);
    return list;
  });
}

export function invalidateImageTags(imageId: string): void {
  tagsCache.delete(imageId);
}
