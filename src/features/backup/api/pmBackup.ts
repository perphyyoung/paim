// pm(prompt-manager) 全量备份导入：类型安全绑定转调，类型重导出保持原别名。
import { commands } from "@/bindings";
import type { PmBackupInfo, PmImportProgress, PmImportSummary } from "@/bindings";

/** 备份内容概览（inspect_pm_backup 返回） */
export type { PmBackupInfo };

/** 导入结果摘要（import_pm_backup 返回） */
export type { PmImportSummary };

/** 导入进度推送（事件 pm-import-progress） */
export type { PmImportProgress };

export function inspectPmBackup(zipPath: string) {
  return commands.inspectPmBackup(zipPath);
}

export function importPmBackup(zipPath: string) {
  return commands.importPmBackup(zipPath);
}
