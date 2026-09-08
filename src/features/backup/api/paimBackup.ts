// paim 自有全量备份导出/导入：类型安全绑定转调，类型重导出保持原别名。
import { commands } from "@/bindings";
import type {
  PaimBackupInfo,
  PaimBackupProgress,
  PaimExportSummary,
  PaimImportSummary,
} from "@/bindings";

/** 备份内容概览（inspect_paim_backup 返回） */
export type { PaimBackupInfo };

/** 导出结果摘要（export_paim_backup 返回） */
export type { PaimExportSummary };

/** 导入结果摘要（import_paim_backup 返回） */
export type { PaimImportSummary };

/** 导出/导入进度推送（事件 paim-backup-progress） */
export type { PaimBackupProgress };

export function inspectPaimBackup(zipPath: string) {
  return commands.inspectPaimBackup(zipPath);
}

export function exportPaimBackup(exportPath: string) {
  return commands.exportPaimBackup(exportPath);
}

export function importPaimBackup(zipPath: string) {
  return commands.importPaimBackup(zipPath);
}
