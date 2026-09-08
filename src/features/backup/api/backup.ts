// 完整备份：导出仅 paim；导入自动识别 paim/pm（后端按 manifest appName 分发）。
import { commands } from "@/bindings";
import type {
  BackupExportSummary,
  BackupImportSummary,
  BackupInfo,
  BackupProgress,
} from "@/bindings";

/** 备份内容概览（inspect_backup 返回；app 字段标识来源 "paim" | "pm"） */
export type { BackupInfo };

/** 导出结果摘要（export_backup 返回） */
export type { BackupExportSummary };

/** 导入结果摘要（import_backup 返回） */
export type { BackupImportSummary };

/** 导出/导入进度推送（事件 backup-progress） */
export type { BackupProgress };

export function inspectBackup(zipPath: string) {
  return commands.inspectBackup(zipPath);
}

export function exportBackup(exportPath: string) {
  return commands.exportBackup(exportPath);
}

export function importBackup(zipPath: string) {
  return commands.importBackup(zipPath);
}
