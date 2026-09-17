import type { ProviderProfile } from "../api/client";

export const PAGES = ["供应商", "客户端通用配置", "扩展", "会话", "用量", "设置"] as const;
export type Page = (typeof PAGES)[number];
export type ProviderView =
  | { kind: "list" }
  | { kind: "import" }
  | { kind: "usage"; profile: ProviderProfile };

export const SETTINGS_SECTIONS = [
  { value: "application", label: "应用偏好" },
  { value: "backups", label: "备份与恢复" },
  { value: "gateway", label: "本机网关" },
  { value: "client-management", label: "客户端管理" },
  { value: "diagnostics", label: "诊断" },
  { value: "about", label: "关于与更新" },
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
] as const;
export type UsageSection = (typeof USAGE_SECTIONS)[number]["value"];
export const DIAGNOSTIC_SECTIONS = [
  { value: "configuration", label: "配置与环境" },
  { value: "logs", label: "运行日志" },
] as const;
export type DiagnosticSection = (typeof DIAGNOSTIC_SECTIONS)[number]["value"];
