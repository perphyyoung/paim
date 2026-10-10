// 跨页数据一致性：页面实例被 KeepAlive 缓存后不会自动重拉。
// 当某页的操作改变了另一页展示的数据时（如上传带提示词的图像、删除/恢复提示词），
// 标记对应页为脏；该页经 usePageStaleReload 在激活（或设置关闭）时消费标记并重载。

export type PageKey = "images" | "prompts";

const stalePages = new Set<PageKey>();

/** 标记某页的缓存数据已过期（下次激活时重载） */
export function markPageStale(page: PageKey): void {
  stalePages.add(page);
}

/** 检查并清除某页的过期标记（返回 true 表示需要重载） */
export function consumePageStale(page: PageKey): boolean {
  return stalePages.delete(page);
}

/**
 * 设置悬浮面板关闭事件：设置内的操作（向量索引/清空、缩略图重建等）会标脏主页，
 * 但设置是遮罩层、KeepAlive 页面不会触发 onActivated；App 在关闭时广播此事件，
 * `usePageStaleReload` 监听后让当前活动页即时消费脏标记重载，非活动页仍在下次激活时懒重载。
 */
export const SETTINGS_CLOSED_EVENT = "paim:settings-closed";
