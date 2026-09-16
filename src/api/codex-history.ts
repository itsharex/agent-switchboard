/*
 * Codex 跨供应商统一历史迁移（S03）：只读桶扫描、带账本的存量迁移、按账本还原。
 * 本模块只做类型化调用；真实写入由后端逐对象备份后完成，前端从不改写会话文件。
 */
import { invoke } from "./client";

/** 统一目标桶：ASB 的 Codex 投影恒为内建 provider，遗留桶都并入它。 */
export const UNIFIED_PROVIDER_ID = "openai";

/** 一个来源桶的存量会话统计（只读扫描结果）。 */
export interface CodexHistoryBucket {
  providerId: string;
  jsonlFiles: number;
  stateRows: number;
}

export interface CodexHistoryBuckets {
  /** 当前 Codex 目录是否已有目录绑定的完成标记。 */
  migrationCompleted: boolean;
  /** 是否存在可还原的迁移备份账本。 */
  backupAvailable: boolean;
  buckets: CodexHistoryBucket[];
}

export interface CodexHistoryUnifyOutcome {
  /** 调用方是否省略了来源（使用已知遗留桶默认集）。 */
  legacyDefault: boolean;
  sourceProviderIds: string[];
  migratedJsonlFiles: number;
  migratedStateRows: number;
  /** 未执行的原因码；`null` 表示确实执行了。 */
  skippedReason: string | null;
}

export interface CodexHistoryRestoreOutcome {
  restoredJsonlFiles: number;
  restoredStateRows: number;
  skippedReason: string | null;
}

export const scanCodexHistoryBuckets = (): Promise<CodexHistoryBuckets> =>
  invoke("scan_codex_history_buckets");

/** `sourceProviderIds: null` 走已知遗留桶默认集；数组则只迁移显式选择的桶。 */
export const migrateCodexHistoryToUnified = (
  sourceProviderIds: string[] | null,
): Promise<CodexHistoryUnifyOutcome> =>
  invoke("migrate_codex_history_to_unified", { sourceProviderIds });

export const hasCodexHistoryUnifyBackup = (): Promise<boolean> =>
  invoke("has_codex_history_unify_backup");

export const restoreCodexHistoryFromBackups = (): Promise<CodexHistoryRestoreOutcome> =>
  invoke("restore_codex_history_from_backups");

const SKIP_REASONS: Record<string, string> = {
  already_migrated: "当前 Codex 目录已迁移过（完成标记存在），本次未重复改写。",
  no_source_provider_ids: "没有可迁移的来源桶，未改动任何会话。",
  no_backup_ledger: "没有可还原的迁移备份账本。",
  nothing_to_restore: "没有需要还原的会话：账本内对象已不在统一桶。",
};

/** 把后端的跳过原因码转成用户可读文本；未知码原样带出而不隐藏。 */
export function describeCodexHistorySkip(reason: string | null): string | null {
  if (!reason) return null;
  return SKIP_REASONS[reason] ?? `未执行：${reason}`;
}
