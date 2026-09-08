import type { SettingSpec, SettingValue } from "../../api/client";
import { Button } from "../Button";
import { Checkbox } from "../Checkbox";
import { Input } from "../Input";
import { ModelPicker } from "../ModelPicker";
import { RadioOption } from "../RadioOption";
import { SettingsFields, SettingsRow } from "../SettingsFields";
import { CONTEXT_WINDOW_1M, codexOptions } from "./draft";
import type { ProviderEditorState } from "./useProviderEditor";

interface Props { editor: ProviderEditorState; busy: boolean }

const automatic: SettingValue = { mode: "automatic" };

function explicit(value: string): SettingValue {
  return { mode: "explicit", value };
}

function textValue(value: SettingValue): string {
  return value.mode === "explicit" && typeof value.value === "string" ? value.value : "";
}

export function ParametersLoadStatus({ editor, busy }: Props) {
  if (editor.parameters.ready) return null;
  return editor.parameters.error ? (
    <div className="asb-field-error" role="alert">
      <p>无法读取运行参数：{editor.parameters.error}</p>
      <Button variant="secondary" disabled={busy} onClick={editor.parameters.retry}>重新读取运行参数</Button>
    </div>
  ) : <p className="asb-field-help" role="status">正在读取运行参数</p>;
}

function CodexContextSettings({ editor, busy }: Props) {
  const { draft, setDraft } = editor;
  return (
    <section className="asb-toggle-group">
      <div className="asb-toggle-group-head"><h3 className="asb-toggle-group-title">上下文窗口</h3></div>
      <div className="asb-toggle-row asb-choice-row">
        <Checkbox label="启用 1M 上下文窗口" ariaLabel="启用 1M 上下文窗口"
          checked={draft.modelOptions?.kind === "codex" && draft.modelOptions.contextWindow === CONTEXT_WINDOW_1M}
          disabled={busy} onChange={(checked) => setDraft((current) => ({
            ...current, modelOptions: codexOptions(current.modelOptions, { contextWindow: checked ? CONTEXT_WINDOW_1M : null }),
          }))} />
      </div>
    </section>
  );
}

function SubagentModelRow({ spec, value, editor, busy }: {
  spec: Extract<SettingSpec, { control: "model" }>;
  value: SettingValue;
  editor: ProviderEditorState;
  busy: boolean;
}) {
  const { connection, draft, setDraft } = editor;
  const current = textValue(value);
  const custom = value.mode === "explicit";
  const setValue = (next: SettingValue) => setDraft((existing) => existing.parameters
    ? { ...existing, parameters: { settings: { ...existing.parameters.settings, [spec.key]: next } } }
    : existing);
  const defaultModel = draft.model?.trim() ?? "";
  return (
    <div className="asb-toggle-row asb-choice-row">
      <div className="asb-choice-head">
        <div>
          <span className="asb-checkbox-label">{spec.label}</span>
          <p className="asb-field-help">从此供应商的模型列表选择；任务或角色显式模型仍可覆盖。</p>
        </div>
      </div>
      <div className="asb-subagent-controls" role="radiogroup" aria-label={`${spec.label}配置方式`}>
        <RadioOption name={`${spec.key}-mode`} checked={!custom} disabled={busy} label="自动"
          onChange={() => setValue(automatic)} />
        <RadioOption name={`${spec.key}-mode`} checked={custom} disabled={busy} label="指定"
          onChange={() => setValue(explicit(current || defaultModel))} />
        {custom && (
          <div className="asb-model-control asb-provider-subagent-model-control">
            <Input code aria-label={`${spec.label}值`} autoComplete="off" disabled={busy}
              value={current} onChange={(event) => setValue(explicit(event.target.value))} />
            {connection.models && (
              <ModelPicker models={connection.models} current={current || null}
                ariaLabel={`选择${spec.label}`} disabled={busy || connection.modelsBusy}
                onSelect={(model) => setValue(explicit(model))} />
            )}
            <Button variant="secondary" disabled={busy || connection.modelsBusy || !connection.baseUrl}
              onClick={() => void connection.fetchModels()}>
              {connection.modelsBusy ? "获取中…" : "获取模型"}
            </Button>
            {connection.modelsError && <span className="asb-warn-text">{connection.modelsError}</span>}
          </div>
        )}
      </div>
    </div>
  );
}

function SubagentSettingsGroup({ specs, editor, busy, onReset, onChange }: {
  specs: SettingSpec[];
  editor: ProviderEditorState;
  busy: boolean;
  onReset: () => void;
  onChange: (key: string, value: SettingValue) => void;
}) {
  if (specs.length === 0 || !editor.draft.parameters) return null;
  const model = specs.find((spec): spec is Extract<SettingSpec, { control: "model" }> => spec.control === "model");
  return (
    <section className="asb-toggle-group">
      <div className="asb-toggle-group-head">
        <h3 className="asb-toggle-group-title">子 agent</h3>
        <Button variant="secondary" disabled={busy} onClick={onReset}>恢复默认值</Button>
      </div>
      <p className="asb-field-help">默认模型与推理强度随此供应商保存，并在切换到它时应用。</p>
      {model && <SubagentModelRow spec={model} value={editor.draft.parameters.settings[model.key]}
        editor={editor} busy={busy} />}
      {specs.filter((spec) => spec.control !== "model").map((spec) => (
        <SettingsRow key={spec.key} spec={spec} value={editor.draft!.parameters!.settings[spec.key]} busy={busy}
          onChange={(value) => onChange(spec.key, value)} />
      ))}
    </section>
  );
}

export function ProviderParametersPage({ editor, busy }: Props) {
  const { draft, setDraft, parameters } = editor;
  const catalog = parameters.catalog;
  if (!catalog || !draft.parameters) return <ParametersLoadStatus editor={editor} busy={busy} />;
  const reset = (group: string | null) => setDraft((current) => {
    if (!current.parameters) return current;
    const settings = { ...current.parameters.settings };
    for (const spec of catalog.specs) {
      if (group === null || spec.group === group) settings[spec.key] = catalog.defaults.settings[spec.key];
    }
    return { ...current, parameters: { settings },
      modelOptions: group === null && current.modelOptions?.kind === "codex" ? null : current.modelOptions };
  });
  const subagentSpecs = draft.app === "codex"
    ? catalog.specs.filter((spec) => spec.group === "子 agent")
    : [];
  const parameterSpecs = catalog.specs.filter((spec) => spec.group !== "子 agent");
  const parameterGroups = catalog.groups.filter((group) => group !== "子 agent");
  const change = (key: string, value: SettingValue) => setDraft((current) => current.parameters
    ? { ...current, parameters: { settings: { ...current.parameters.settings, [key]: value } } }
    : current);
  return (
    <div className="asb-form" aria-label="供应商运行参数">
      <p className="asb-field-help">运行参数随此供应商保存，返回编辑后统一保存供应商。</p>
      {draft.app === "codex" && draft.routeMode === "custom" && <CodexContextSettings editor={editor} busy={busy} />}
      <SettingsFields specs={parameterSpecs} groups={parameterGroups} values={draft.parameters.settings} busy={busy}
        onResetGroup={reset} onChange={change} />
      <SubagentSettingsGroup specs={subagentSpecs} editor={editor} busy={busy}
        onReset={() => reset("子 agent")} onChange={change} />
      <div className="asb-settings-actions">
        <Button variant="secondary" disabled={busy} onClick={() => reset(null)}>全部恢复默认值</Button>
      </div>
    </div>
  );
}
