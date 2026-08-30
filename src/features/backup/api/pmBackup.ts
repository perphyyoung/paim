// pm(prompt-manager) 全量备份导入：类型定义与命令封装。
import { invoke } from "@tauri-apps/api/core";

/** 备份内容概览（inspect_pm_backup 返回） */
export interface PmBackupInfo {
  exported_at: string;
  prompt_count: number;
  image_count: number;
  trashed_image_count: number;
  prompt_tag_count: number;
  image_tag_count: number;
}

/** 导入结果摘要（import_pm_backup 返回） */
export interface PmImportSummary {
  prompts: number;
  images: number;
  thumbnail_failures: number;
  backup_db: string;
  backup_images_dir: string | null;
}

/** 导入进度推送（事件 pm-import-progress） */
export interface PmImportProgress {
  stage: string;
  percent: number;
  status: string;
  detail: string | null;
}

export function inspectPmBackup(zipPath: string) {
  return invoke<PmBackupInfo>("inspect_pm_backup", { zipPath });
}

export function importPmBackup(zipPath: string) {
  return invoke<PmImportSummary>("import_pm_backup", { zipPath });
}
