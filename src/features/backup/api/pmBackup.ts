// pm(prompt-manager) 全量备份导入：类型安全绑定转调；类型复用 bindings 中的共享 Backup*（pm/paim 同构）。
import { commands } from "@/bindings";
import type { BackupInfo, BackupImportSummary, BackupProgress } from "@/bindings";

/** 备份内容概览（inspect_pm_backup 返回） */
export type { BackupInfo };

/** 导入结果摘要（import_pm_backup 返回） */
export type { BackupImportSummary };

/** 导入进度推送（事件 backup-progress） */
export type { BackupProgress };

export function inspectPmBackup(zipPath: string) {
  return commands.inspectPmBackup(zipPath);
}

export function importPmBackup(zipPath: string) {
  return commands.importPmBackup(zipPath);
}
