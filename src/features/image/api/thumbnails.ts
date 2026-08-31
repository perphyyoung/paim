// 缩略图重建（设置页入口）：类型定义与命令封装。
import { invoke } from "@tauri-apps/api/core";

/** 全量重建结果摘要（rebuild_thumbnails 返回；success 含已存在跳过与新生成两类） */
export interface ThumbnailRebuildSummary {
  total: number;
  success: number;
  failed: number;
}

/** 重建进度推送（事件 thumbnail-rebuild-progress） */
export interface ThumbnailRebuildProgress {
  current: number;
  total: number;
  file_name: string;
}

export function rebuildThumbnails() {
  return invoke<ThumbnailRebuildSummary>("rebuild_thumbnails");
}

/** 懒自愈单条修复结果 */
export interface ThumbnailEnsureFixed {
  id: string;
  thumbnail_path: string;
}

/** 懒自愈结果（ensure_image_thumbnails 返回） */
export interface ThumbnailEnsureResult {
  fixed: ThumbnailEnsureFixed[];
  missing: string[];
}

/** 批量校验指定图像的缩略图文件，缺失且原图存在时按需生成并回写 */
export function ensureImageThumbnails(ids: string[]) {
  return invoke<ThumbnailEnsureResult>("ensure_image_thumbnails", { ids });
}
