// 提示词关联图像的实体级缓存。
// 详情弹窗被父级 v-if 强制卸载（PromptPage 每次打开都是新实例），组件内 ref 存不住，
// 所以缓存必须挂在模块作用域：切换/重开同一条提示词时不再重复读库。
// 失效只在「关联关系变化」的操作上做（移除/设为首图/导入/替换），保存提示词字段不影响。

import { commands, type RelatedImage } from "@/bindings";

const cache = new Map<string, RelatedImage[]>();
// 并发去重：同一 promptId 的多个调用共用一个 in-flight Promise，只发一次命令
const inflight = new Map<string, Promise<RelatedImage[]>>();

export function getCachedRelatedImages(promptId: string): RelatedImage[] | undefined {
  return cache.get(promptId);
}

/** 读取并在回填缓存；未命中才发命令 */
export function fetchRelatedImages(promptId: string): Promise<RelatedImage[]> {
  const pending = inflight.get(promptId);
  if (pending) return pending;
  const task = commands
    .getPromptRelatedImages(promptId)
    .then((list) => {
      cache.set(promptId, list);
      return list;
    })
    .finally(() => inflight.delete(promptId));
  inflight.set(promptId, task);
  return task;
}

/** 关联关系已变化，丢弃该条缓存（下次展示重新读库） */
export function invalidateRelatedImages(promptId: string): void {
  cache.delete(promptId);
}
