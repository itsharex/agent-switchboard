import type { AppKind } from "../shared";

/* ---------------------------------------------------------------- Extensions
 * workspace (Skills / MCP). Types mirror the serde camelCase DTOs in
 * crates/asb-core/src/extensions; commands live in
 * src-tauri/src/commands/extensions.rs. */

export type ExtensionKind = "skill" | "mcp";

export type ExtensionTarget =
  | { scope: "app"; client: AppKind }
  | { scope: "projectShared"; client: AppKind; projectId: string }
  | { scope: "projectPrivate"; client: AppKind; projectId: string };

/** One value position that can carry sensitive material. */
export type SecretValue =
  | { mode: "envRef"; name: string }
  | { mode: "secretRef"; reference: string }
  | { mode: "plain"; value: string };

export interface SkillManifest {
  name: string;
  description?: string | null;
  license?: string | null;
  allowedTools?: string[] | null;
  unparsedKeys?: string[];
}

export interface SourceRef {
  sourceId: string;
  subpath: string;
  refName?: string | null;
  resolvedCommit?: string | null;
}

export interface SkillDependency {
  name: string;
  resourceId?: string | null;
}

export interface CompatibilityNote {
  code: string;
  message: string;
}

export interface SkillDefinition {
  contentDigest: string;
  manifest: SkillManifest;
  source?: SourceRef | null;
  hostScoped?: AppKind | null;
  compatibility?: CompatibilityNote[];
  dependencies?: SkillDependency[];
}

export interface CodexServerOptions {
  cwd?: string | null;
  startupTimeoutSec?: number | null;
  toolTimeoutSec?: number | null;
  required?: boolean | null;
}

export type McpDefinition =
  | {
      transport: "stdio";
      command: string;
      args?: string[];
      env?: Record<string, SecretValue>;
      codexOptions?: CodexServerOptions | null;
    }
  | {
      transport: "http";
      url: string;
      headers?: Record<string, SecretValue>;
      bearer?: SecretValue | null;
    }
  | {
      transport: "claudeSse";
      url: string;
      headers?: Record<string, SecretValue>;
    }
  | {
      transport: "claudeWs";
      url: string;
      headers?: Record<string, SecretValue>;
    };

export type ExtensionPayload =
  | ({ kind: "skill" } & SkillDefinition)
  | ({ kind: "mcp" } & McpDefinition);

export interface ExtensionDefinitionEnvelope {
  schemaVersion: number;
  id: string;
  name: string;
  revision: number;
  createdAt: string;
  updatedAt: string;
}

export type ExtensionDefinition = ExtensionDefinitionEnvelope & ExtensionPayload;

export type DesiredState = "enabled" | "disabled";

export interface ExtensionBinding {
  schemaVersion: number;
  id: string;
  resourceId: string;
  target: ExtensionTarget;
  nativeKey?: string | null;
  deployName?: string | null;
  desired: DesiredState;
  lockedDigest?: string | null;
  lastAppliedRevision?: number | null;
  updatedAt: string;
}

export type FileState =
  | "notDeployed"
  | "inSync"
  | "pendingApply"
  | "externalChange"
  | "missing"
  | "unreadable";

export type BindingStatus = ExtensionBinding & {
  fileState: FileState;
  warnings: string[];
};

export type DependencyState = "bound" | "pendingConfiguration" | "targetUnsupported";

export interface DependencyStatus {
  name: string;
  resourceId: string | null;
  state: DependencyState;
}

/** Safe renderer projection of a credential position. This is intentionally
 * separate from SecretValue, which is accepted only when saving a draft. */
export type SecretValueView =
  | { mode: "envRef"; name: string }
  | { mode: "stored" }
  | { mode: "redacted" };

/** Source identity and path never leave the extension backend. */
export interface SkillSourceView {
  resolvedCommit?: string | null;
}

type ExtensionListItemCommon = ExtensionDefinitionEnvelope & {
  bindings: BindingStatus[];
  dependencyStates: DependencyStatus[];
  lastCheck: McpCheckResult | null;
};

export type ExtensionListItem =
  | (ExtensionListItemCommon & {
      kind: "skill";
      contentDigest: string;
      manifest: SkillManifest;
      source: SkillSourceView | null;
      hostScoped: AppKind | null;
      compatibility: CompatibilityNote[];
      /** Declared dependencies with their library MCP links; states derive
       * per target at read time. */
      dependencies: SkillDependency[];
    })
  | (ExtensionListItemCommon & {
      kind: "mcp";
      transport: "stdio";
      command: string;
      argumentCount: number;
      env: Record<string, SecretValueView>;
    })
  | (ExtensionListItemCommon & {
      kind: "mcp";
      transport: "http" | "claudeSse" | "claudeWs";
      /** Always the stable redaction marker, never the upstream address. */
      url: string;
      headers: Record<string, SecretValueView>;
      bearer?: SecretValueView;
    });

/** The safe result from an extension-library mutation. */
export interface ExtensionMutation {
  id: string;
  name: string;
  revision: number;
}

export interface ProjectRegistration {
  schemaVersion: number;
  id: string;
  root: string;
  displayName: string;
  registeredAt: string;
}

export type ObservedOrigin =
  | { origin: "userRoot" }
  | { origin: "legacyRoot" }
  | { origin: "projectRoot" }
  | { origin: "managed" };

export interface ObservedExtension {
  observationId: string;
  kind: ExtensionKind;
  client: AppKind;
  name: string;
  description?: string | null;
  origin: ObservedOrigin;
  managed: boolean;
  contentDigest?: string | null;
  transport?: string | null;
  diagnostics: string[];
}

export interface ExtensionDiscoveryDiagnostic {
  client: AppKind;
  message: string;
}

export interface ExtensionDiscovery {
  observations: ObservedExtension[];
  diagnostics: ExtensionDiscoveryDiagnostic[];
}

export interface SkillCandidateDto {
  digest: string;
  name: string;
  description: string | null;
  fileCount: number;
  diagnostics: string[];
}

export type SkillFileChangeAction = "added" | "removed" | "modified";

export interface SkillFileChange {
  relativePath: string;
  action: SkillFileChangeAction;
}

/** One skill's source-freshness result. Batch checks keep going when a
 * single skill fails; that failure lands in `error` instead of aborting
 * the whole request. */
export interface SkillUpdateReport {
  definitionId: string;
  upToDate: boolean;
  currentCommit: string | null;
  newCommit: string | null;
  newDigest: string | null;
  changedFiles: SkillFileChange[];
  error: string | null;
}

export type CapabilityVerification =
  | { verification: "verified"; clientVersion: string; verifiedOn: string }
  | { verification: "open"; condition: string };

export type CapabilityEntry = {
  code: string;
  resource: string;
  supported: boolean;
} & CapabilityVerification;

export interface ClientCapabilityReport {
  client: AppKind;
  entries: CapabilityEntry[];
}

export type McpCheckOutcome =
  | { kind: "passed"; tools: number; resources: number; prompts: number }
  | { kind: "partial"; error: string }
  | { kind: "failed"; classification: string; error: string }
  | { kind: "cancelled" }
  | { kind: "needsNativeConfirmation" };

export interface McpCheckResult {
  definitionId: string;
  definitionRevision: number;
  target: ExtensionTarget;
  checkedAt: string;
  protocolVersion?: string | null;
  outcome: McpCheckOutcome;
  durationMs: number;
  truncated: boolean;
}

export type TargetOutcome =
  | { kind: "applied" }
  | { kind: "failed"; message: string }
  | { kind: "restored"; message: string }
  | { kind: "restoreFailed"; message: string; backup: string }
  | { kind: "skipped"; message: string };

export interface ExtensionTargetResult {
  target: ExtensionTarget;
  outcome: TargetOutcome;
  warnings?: string[];
}

export interface RollbackSummary {
  restored: string[];
  failed: string[];
}

export type PlanOperation = "install" | "update" | "enable" | "disable" | "remove" | "restore";

/** One resource's slice of a finished batch operation. */
export interface OperationResourceRecord {
  definitionId: string;
  definitionRevision: number;
  operation: PlanOperation;
  targets: ExtensionTargetResult[];
}

export interface ExtensionOperationRecord {
  schemaVersion: number;
  id: string;
  createdAt: string;
  finishedAt?: string | null;
  resources: OperationResourceRecord[];
  rollback?: RollbackSummary | null;
}

export interface PlanChangeView {
  pointer: string;
  before?: string | null;
  after?: string | null;
}

export type FileAction = "deploy" | "remove";

export interface FileChangeView {
  relativePath: string;
  action: FileAction;
}

export interface PlannedTargetView {
  target: ExtensionTarget;
  warnings: string[];
  changes: PlanChangeView[];
  files?: FileChangeView[];
  writesSensitiveConnectionData: boolean;
}

export interface PlannedOperationView {
  definitionId: string;
  definitionRevision: number;
  operation: PlanOperation;
  targets: PlannedTargetView[];
}

/** The redacted batch plan preview, grouped by resource operation. */
export interface ExtensionPlanView {
  planId: string;
  createdAt: string;
  expiresAt: string;
  operations: PlannedOperationView[];
}

export interface ExtensionsWorkspace {
  generation: number;
  items: ExtensionListItem[];
  projects: ProjectRegistration[];
  history: ExtensionOperationRecord[];
  capabilities: ClientCapabilityReport[];
  recoveryRequired: string[];
}

export interface ExtensionDraft {
  name: string;
  payload: ExtensionPayload;
}

/** One editable secret-bearing position as prefilled for the editor. Stored
 * credentials are reduced to a presence marker; their values and handles
 * never cross the IPC boundary back to the renderer. */
export type SecretSlotView =
  | { mode: "plain"; value: string }
  | { mode: "envRef"; name: string }
  | { mode: "secretConfigured" };

export interface SecretSlot {
  name: string;
  value: SecretSlotView;
}

export type McpEditView =
  | {
      transport: "stdio";
      command: string;
      args: string[];
      env: SecretSlot[];
      codexOptions: CodexServerOptions | null;
    }
  | {
      transport: "http";
      url: string;
      headers: SecretSlot[];
      bearer: SecretSlotView | null;
    }
  | { transport: "claudeSse"; url: string; headers: SecretSlot[] }
  | { transport: "claudeWs"; url: string; headers: SecretSlot[] };

export type McpEditViewEnvelope = {
  id: string;
  revision: number;
  name: string;
} & McpEditView;

/** One field position's explicit treatment in an edit: absent keeps the
 * stored value verbatim (kept values never round-trip through the
 * renderer), replace overwrites, delete removes optional positions. */
export type FieldEdit<T> = { action: "replace"; value: T } | { action: "delete" };

export interface McpFieldEdits {
  command?: FieldEdit<string>;
  args?: FieldEdit<string[]>;
  env?: Record<string, FieldEdit<SecretValue>>;
  url?: FieldEdit<string>;
  headers?: Record<string, FieldEdit<SecretValue>>;
  bearer?: FieldEdit<SecretValue>;
  codexOptions?: FieldEdit<CodexServerOptions>;
}

export interface McpEditRequest {
  expectedRevision: number;
  serverKey?: string;
  transport?: McpDefinition;
  fields?: McpFieldEdits;
}

/** One typed resource operation inside a batch plan request: a
 * single-resource request is a batch of one. */
export interface PlanRequestOperation {
  operation: PlanOperation;
  definitionId?: string | null;
  bindingId?: string | null;
  targets?: ExtensionTarget[];
  sharedSettings?: boolean | null;
}

export interface PlanRequest {
  operations: PlanRequestOperation[];
}

export interface ApplyOutcome {
  record: ExtensionOperationRecord | null;
  rejected: string | null;
  rolledBack: boolean;
}
