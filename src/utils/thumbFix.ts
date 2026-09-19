// 提示词/图像主页共用的缩略图 URL 处理。
import type { ThumbnailEnsureFixed } from "@/bindings";
import { toAssetUrl } from "@/utils/assetUrl";

/**
 * 把懒自愈重建的缩略图并入 URL 映射，返回新映射（不改入参）。
 * cache-buster：自愈重建后 DB 路径不变 → URL 全等会让 Vue 跳过 <img> patch，
 * 浏览器不重新请求、卡片背景停留在失败状态；拼时间戳强制 URL 变化触发重载
 * （asset 协议按 path 服务、忽略 query）。提示词/图像主页共用。
 */
export function applyThumbFix(
  dir: string,
  cur: Record<string, string>,
  fixed: ThumbnailEnsureFixed[],
): Record<string, string> {
  const map = { ...cur };
  for (const f of fixed) map[f.id] = `${toAssetUrl(`${dir}/${f.thumbnail_path}`)}?t=${Date.now()}`;
  return map;
}
