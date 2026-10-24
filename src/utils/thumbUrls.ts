// 只有 id、没有实体行时的缩略图 URL 解析：图像 / 提示词两侧同形。
// 两侧各一条「取相对路径」命令（get_image_thumbs / get_prompt_thumbs，都返回 `thumbnail_path` 原值）
// + 一条同名自愈命令（ensure_image_thumbnails / ensure_prompt_thumbnails），最后统一拼 asset URL。
//
// 跨组件只传 id、不传图片 URL：详情侧手里是磁盘绝对路径，直接当 `<img src>` 会破图
// （历史问题见 docs/lessons.md 第 36 节）。
import { commands } from "@/bindings";
import { relativePathToAssetUrl } from "@/utils/assetUrl";
import { applyThumbFix } from "@/utils/thumbFix";

export type ThumbKind = "image" | "prompt";

/**
 * 按 id 批量解析缩略图 asset URL：{id: url}。
 * 缺图的条目走一次懒自愈（与主页同一套：重建后拼 cache-buster，让 `<img>` 重新请求）；
 * 仍然取不到的 id 不出现在返回值里，调用方按「无图」处理。
 */
export async function resolveThumbUrls(
  kind: ThumbKind,
  ids: string[],
): Promise<Record<string, string>> {
  if (ids.length === 0) return {};
  const dir = await commands.getDataDir();
  const rel =
    kind === "image" ? await commands.getImageThumbs(ids) : await commands.getPromptThumbs(ids);
  const map: Record<string, string> = {};
  for (const [id, path] of Object.entries(rel)) map[id] = relativePathToAssetUrl(dir, path);
  const missing = ids.filter((id) => !map[id]);
  if (missing.length > 0) {
    const fixed =
      kind === "image"
        ? await commands.ensureImageThumbnails(missing)
        : await commands.ensurePromptThumbnails(missing);
    Object.assign(map, applyThumbFix(dir, {}, fixed.fixed));
  }
  return map;
}
