import type {
  AppKind,
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
};

export const TRANSPORT_LABELS: Record<string, string> = {
  stdio: "stdio",
  http: "HTTP",
  claudeSse: "SSE（仅 Claude）",
  claudeWs: "WebSocket（仅 Claude）",
};

/** The MCP transports project to Codex as well; Claude-only ones do not. */
export function mcpSupportsClient(transport: string, client: AppKind): boolean {
  if (transport === "claudeSse" || transport === "claudeWs") return client === "claude";
  return true;
}

export function targetLabel(
  target: ExtensionTarget,
  projectNames?: ReadonlyMap<string, string>,
): string {
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
