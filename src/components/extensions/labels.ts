import type {
  AppKind,
  ExtensionDiagnosticRemediation,
  ExtensionListItem,
  ExtensionTarget,
  FileState,
  PlanOperation,
  SkillFileChange,
  TargetOutcome,
} from "../../api/client";
import { clientName } from "../../lib/client-name";

export const FILE_STATE_LABELS: Record<FileState, string> = {
  notDeployed: "未部署",
  inSync: "已一致",
  pendingApply: "待应用",
  externalChange: "外部改动",
  missing: "文件缺失",
  unreadable: "不可读",
};

export const SKILL_CHANGE_LABELS: Record<SkillFileChange["action"], string> = {
  added: "新增",
  removed: "移除",
  modified: "修改",
};

export const OPERATION_LABELS: Record<PlanOperation, string> = {
  install: "安装",
  update: "更新",
  enable: "启用",
  disable: "停用",
  remove: "移除",
  restore: "恢复",
  repair: "修复",
};

/** Renderer-safe labels for diagnostic problem codes. */
export const DIAGNOSTIC_CODE_LABELS: Record<string, string> = {
  skillManifestMissing: "缺少 SKILL.md",
  skillFrontmatterMissing: "缺少 frontmatter",
  skillFrontmatterInvalid: "frontmatter 无法解析",
  skillDirUnreadable: "目录无法读取",
  skillEntryLink: "包含链接或重解析点",
  skillEntryUnsupported: "包含不支持的文件类型",
  skillRootUnreadable: "Skill 根目录无法读取",
  skillRootEntryUnreadable: "Skill 目录项无法读取",
  mcpDocumentUnreadable: "MCP 文档无法读取",
  mcpDocumentUnparsable: "MCP 文档无法解析",
  mcpCollectionInvalid: "MCP 集合类型错误",
  mcpEntryNotAnObject: "MCP 条目不是对象",
  mcpTransportMissing: "缺少传输字段",
  mcpTransportConflicting: "传输字段冲突",
  mcpTransportUnknown: "未知传输类型",
  mcpUnknownFields: "未识别字段（按原文保留）",
  mcpIgnoredField: "该客户端忽略的字段",
  managedTargetMissing: "托管目标缺失",
  managedEntryMissing: "托管条目缺失",
  managedTargetExternalChange: "外部改动",
  managedTargetUnreadable: "托管目标不可读",
};

export const REMEDIATION_LABELS: Record<ExtensionDiagnosticRemediation["kind"], string> = {
  auto: "可自动修复",
  manual: "需人工处理",
  info: "信息提示",
};

export const TRANSPORT_LABELS: Record<string, string> = {
  stdio: "stdio",
  http: "HTTP",
  claudeSse: "SSE（仅 Claude）",
  claudeWs: "WebSocket（仅 Claude）",
};

/** Worst-state-first ordering: the summary names the state that needs
 * attention before the reassuring ones. */
const FILE_STATE_SEVERITY: Array<ExtensionListItem["bindings"][number]["fileState"]> = [
  "unreadable",
  "externalChange",
  "missing",
  "pendingApply",
  "notDeployed",
  "inSync",
];

/** One line of text for an item's deployment state on one client; the row
 * toggles carry it as their accessible name so state never rides on colour
 * alone. */
export function clientSummary(item: ExtensionListItem, client: AppKind): string {
  const rows = item.bindings.filter((binding) => binding.target.client === client);
  if (rows.length === 0) return "未部署";
  const worst =
    FILE_STATE_SEVERITY.find((state) => rows.some((binding) => binding.fileState === state)) ?? "inSync";
  const enabled = rows.filter((binding) => binding.desired === "enabled").length;
  if (enabled > 0 && enabled < rows.length) return `${FILE_STATE_LABELS[worst]} · 部分启用`;
  return enabled === 0 ? `${FILE_STATE_LABELS[worst]} · 已停用` : FILE_STATE_LABELS[worst];
}

export function targetLabel(target: ExtensionTarget, projectNames?: ReadonlyMap<string, string>): string {
  const client = clientName(target.client);
  if (target.scope === "app") return `${client} 用户配置`;
  const project = projectNames?.get(target.projectId);
  const scope = target.scope === "projectShared" ? "项目共享" : "项目私有";
  return project ? `${project} · ${client} ${scope}` : `${client} ${scope}`;
}

/** Encodes one target for the Select control; the client stays a plain
 * string because Select options are string-keyed. */
export function targetValue(target: ExtensionTarget): string {
  if (target.scope === "app") return `app:${target.client}`;
  return `${target.scope}:${target.client}:${target.projectId}`;
}

export function parseTargetValue(value: string): ExtensionTarget | null {
  const [scope, client, projectId] = value.split(":");
  if (scope === "app" && (client === "codex" || client === "claude")) {
    return { scope: "app", client };
  }
  if (
    (scope === "projectShared" || scope === "projectPrivate") &&
    (client === "codex" || client === "claude") &&
    projectId
  ) {
    return { scope, client, projectId };
  }
  return null;
}

export function outcomeText(outcome: TargetOutcome): string {
  switch (outcome.kind) {
    case "applied":
      return "已应用";
    case "failed":
      return `失败：${outcome.message}`;
    case "restored":
      return `已恢复：${outcome.message}`;
    case "restoreFailed":
      return `恢复失败：${outcome.message}（备份 ${outcome.backup}）`;
    case "skipped":
      return `已跳过：${outcome.message}`;
  }
}
