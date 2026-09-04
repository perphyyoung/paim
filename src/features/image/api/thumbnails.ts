// 缩略图重建（设置页入口）：类型安全绑定转调，类型重导出保持原别名。
import { commands } from "@/bindings";
import type {
  ThumbnailEnsureFixed,
  ThumbnailEnsureResult,
  ThumbnailRebuildProgress,
  ThumbnailRebuildSummary,
} from "@/bindings";

/** 全量重建结果摘要（rebuild_thumbnails 返回；success 含已存在跳过与新生成两类） */
export type { ThumbnailRebuildSummary };

/** 重建进度推送（事件 thumbnail-rebuild-progress） */
export type { ThumbnailRebuildProgress };

export function rebuildThumbnails() {
  return commands.rebuildThumbnails();
}

/** 懒自愈单条修复结果 */
export type { ThumbnailEnsureFixed };

/** 懒自愈结果（ensure_image_thumbnails 返回） */
export type { ThumbnailEnsureResult };

/** 批量校验指定图像的缩略图文件，缺失且原图存在时按需生成并回写 */
export function ensureImageThumbnails(ids: string[]) {
  return commands.ensureImageThumbnails(ids);
}
