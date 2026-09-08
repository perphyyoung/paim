// paim 自有全量备份导出/导入：类型安全绑定转调；类型复用 bindings 中的共享 Backup*（pm/paim 同构）。
import { commands } from "@/bindings";
import type {
  BackupExportSummary,
  BackupImportSummary,
  BackupInfo,
  BackupProgress,
} from "@/bindings";

/** 备份内容概览（inspect_paim_backup 返回） */
export type { BackupInfo };

/** 导出结果摘要（export_paim_backup 返回） */
export type { BackupExportSummary };

/** 导入结果摘要（import_paim_backup 返回） */
export type { BackupImportSummary };

/** 导出/导入进度推送（事件 backup-progress） */
export type { BackupProgress };

export function inspectPaimBackup(zipPath: string) {
  return commands.inspectPaimBackup(zipPath);
}

export function exportPaimBackup(exportPath: string) {
  return commands.exportPaimBackup(exportPath);
}

export function importPaimBackup(zipPath: string) {
  return commands.importPaimBackup(zipPath);
}
