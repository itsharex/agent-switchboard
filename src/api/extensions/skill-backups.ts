import { invoke } from "../client";
import type { AppKind } from "../shared";
import type { ExtensionMutation } from "./types";

export interface SkillBackup {
  id: string;
  definitionId: string;
  name: string;
  description: string | null;
  hostScoped: AppKind | null;
  contentDigest: string;
  deletedAt: string;
}

export const listSkillBackups = () => invoke<SkillBackup[]>("list_skill_backups");
export const restoreSkillBackup = (backupId: string) =>
  invoke<ExtensionMutation>("restore_skill_backup", { backupId, confirmWrite: true });
export const deleteSkillBackup = (backupId: string) =>
  invoke<void>("delete_skill_backup", { backupId, confirmWrite: true });
