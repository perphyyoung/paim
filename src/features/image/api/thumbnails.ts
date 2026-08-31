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
