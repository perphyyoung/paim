/**
 * 图像原图路径的共享缓存（详情弹窗 / 全屏查看器 / 主页悬停预览共用）。
 *
 * 键是图像 id，值是 `getImageSrc` 返回的绝对路径（LRU 300，同 id 并发只发一次命令）。
 * 只缓存**路径**，不缓存位图：解码后的位图由浏览器按 asset URL 自行缓存，
 * 自建位图缓存会与浏览器各存一份，内存翻倍且淘汰不可控。
 */
import { commands } from "@/bindings";
import { absolutePathToAssetUrl } from "@/utils/assetUrl";
import { createEntityCache } from "@/utils/entityCache";

const imageSrcCache = createEntityCache<string>((id) => commands.getImageSrc(id), 300);

/** 命中缓存即同步返回原图 asset URL（未命中返回 null），用于「先渲染已缓存的图」 */
export function peekImageSrc(id: string): string | null {
  const cached = imageSrcCache.get(id);
  return cached ? absolutePathToAssetUrl(cached) : null;
}

/** 按 id 取原图 asset URL：命中缓存直接返回；未命中发命令（同 id 并发去重）后回填 */
export async function resolveImageSrc(id: string): Promise<string> {
  const cached = peekImageSrc(id);
  if (cached) return cached;
  return absolutePathToAssetUrl(await imageSrcCache.fetch(id));
}

/** 图像被替换 / 彻底删除后丢弃该条路径（id 复用或文件已移走时避免命中旧路径） */
export function invalidateImageSrc(id: string): void {
  imageSrcCache.invalidate(id);
}

/** 清空回收站等批量删除后整池丢弃（无法精确枚举受影响的 id） */
export function clearImageSrcCache(): void {
  imageSrcCache.clear();
}
