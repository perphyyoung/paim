// 跨页数据一致性：页面实例被 KeepAlive 缓存后不会自动重拉。
// 当某页的操作改变了另一页展示的数据时（如上传带提示词的图像、删除/恢复提示词），
// 标记对应页为脏；该页在 onActivated 时消费标记并重载。

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
