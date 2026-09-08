import type { AppKind, ProviderParametersCatalog, SettingSpec, SettingsValues } from "../api/client";

const codexSpecs: SettingSpec[] = [
  { key: "model_reasoning_effort", label: "推理强度", group: "模型行为", control: "slider",
    options: [{ value: "low", label: "低" }, { value: "medium", label: "中" }, { value: "high", label: "高" }] },
  { key: "hide_agent_reasoning", label: "隐藏推理摘要", group: "模型行为", control: "toggle", options: [] },
  { key: "agents.default_subagent_model", label: "默认子 agent 模型", group: "子 agent", control: "model", options: [] },
  { key: "agents.default_subagent_reasoning_effort", label: "默认推理强度", group: "子 agent", control: "slider",
    options: [{ value: "minimal", label: "极低" }, { value: "low", label: "低" }, { value: "medium", label: "中" }, { value: "high", label: "高" }, { value: "xhigh", label: "极高" }] },
  { key: "features.fast_mode", label: "快速模式", group: "工具与功能", control: "toggle", options: [] },
];
const claudeSpecs: SettingSpec[] = [
  { key: "effortLevel", label: "推理强度", group: "模型行为", control: "slider",
    options: [{ value: "low", label: "低" }, { value: "medium", label: "中" }, { value: "high", label: "高" }] },
  { key: "alwaysThinkingEnabled", label: "扩展思考", group: "模型行为", control: "toggle", options: [] },
];

export function providerParameters(app: AppKind): SettingsValues {
  const specs = app === "codex" ? codexSpecs : claudeSpecs;
  return { settings: Object.fromEntries(specs.map((spec) => [spec.key, { mode: "automatic" }])) };
}

export function providerParametersCatalog(app: AppKind): ProviderParametersCatalog {
  const specs = app === "codex" ? codexSpecs : claudeSpecs;
  return { app, defaults: providerParameters(app), specs, groups: [...new Set(specs.map((spec) => spec.group))] };
}
