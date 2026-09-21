import type { ProviderProfile, WorkspacePage } from "../api/client";

export const WORKSPACE_PAGE_LABELS = {
  providers: "供应商切换", clientConfiguration: "客户端配置", extensions: "扩展",
  sessions: "会话记录", usage: "用量监控", settings: "设置",
} as const satisfies Record<WorkspacePage, string>;
export type Page = (typeof WORKSPACE_PAGE_LABELS)[WorkspacePage];
export const PAGES = Object.values(WORKSPACE_PAGE_LABELS);
export function workspacePageId(page: Page): WorkspacePage {
  return (Object.keys(WORKSPACE_PAGE_LABELS) as WorkspacePage[])
    .find((id) => WORKSPACE_PAGE_LABELS[id] === page)!;
}
export type ProviderView =
  | { kind: "list" }
  | { kind: "import" }
  | { kind: "usage"; profile: ProviderProfile };

export const SETTINGS_SECTIONS = [
  { value: "application", label: "偏好设置" },
  { value: "client-management", label: "客户端工具" },
  { value: "gateway", label: "本机网关" },
  { value: "backups", label: "备份恢复" },
  { value: "diagnostics", label: "诊断" },
  { value: "about", label: "关于" },
] as const;
export type SettingsSection = (typeof SETTINGS_SECTIONS)[number]["value"];

export const EXTENSION_SECTIONS = [
  { value: "skill", label: "Skills" },
  { value: "mcp", label: "MCP" },
] as const;
export type ExtensionSection = (typeof EXTENSION_SECTIONS)[number]["value"];
export const USAGE_SECTIONS = [
  { value: "consumption", label: "消耗统计" },
  { value: "quota", label: "额度与重置" },
  { value: "radar", label: "降智雷达" },
] as const;
export type UsageSection = (typeof USAGE_SECTIONS)[number]["value"];
export const DIAGNOSTIC_SECTIONS = [
  { value: "configuration", label: "配置与环境" },
  { value: "logs", label: "运行日志" },
] as const;
export type DiagnosticSection = (typeof DIAGNOSTIC_SECTIONS)[number]["value"];
