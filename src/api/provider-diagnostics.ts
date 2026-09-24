import { invoke } from "./client";
import type { AppKind } from "./shared";
import type { FilePreview, KeyChange, LocalizedMessage, RestoreOutcome, SwitchOutcome } from "./switching";

export type DiagnosticCategory = "address" | "protocol" | "authentication" | "model" | "environment" | "route" | "configuration";
export type DiagnosticStatus = "pass" | "warning" | "error" | "unknown";
export interface ProviderDiagnosticCheck {
  category: DiagnosticCategory;
  status: DiagnosticStatus;
  message: LocalizedMessage;
  suggestion: LocalizedMessage | null;
}
export interface ProviderDiagnosticsReport {
  profileId: string;
  app: AppKind;
  profileName: string;
  official: boolean;
  active: boolean;
  endpoint: string | null;
  checks: ProviderDiagnosticCheck[];
  canRepair: boolean;
}
export interface ProviderRepairPreview { preparationId: string; file: FilePreview }
export interface ProviderRepairResult { repairId: string; canUndo: boolean; outcome: SwitchOutcome }
export interface ProviderRepairUndoPreview { preparationId: string; changes: KeyChange[] }

export function diagnoseProvider(profileId: string): Promise<ProviderDiagnosticsReport> {
  return invoke("diagnose_provider", { profileId });
}
export function prepareProviderRepair(profileId: string): Promise<ProviderRepairPreview> {
  return invoke("prepare_provider_repair", { profileId });
}
export function commitProviderRepair(preparationId: string, confirmWrite: boolean): Promise<ProviderRepairResult> {
  return invoke("commit_provider_repair", { preparationId, confirmWrite });
}
export function prepareProviderRepairUndo(repairId: string): Promise<ProviderRepairUndoPreview> {
  return invoke("prepare_provider_repair_undo", { repairId });
}
export function commitProviderRepairUndo(preparationId: string, confirmWrite: boolean): Promise<RestoreOutcome> {
  return invoke("commit_provider_repair_undo", { preparationId, confirmWrite });
}
export function cancelProviderRepairPreparation(preparationId: string): Promise<void> {
  return invoke("cancel_provider_repair_preparation", { preparationId });
}
