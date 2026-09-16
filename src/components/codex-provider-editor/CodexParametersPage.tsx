import type { SettingSpec, SettingValue } from "../../api/client";
import { Button } from "../Button";
import { Input } from "../Input";
import { ModelPicker } from "../ModelPicker";
import { RadioOption } from "../RadioOption";
import { SettingsFields, SettingsRow } from "../SettingsFields";
import { ParametersLoadStatus } from "../provider-editor/ProviderParametersPage";
import type { CodexEditorState } from "./useCodexProviderEditor";

interface Props { editor: CodexEditorState; busy: boolean }

const automatic: SettingValue = { mode: "automatic" };

function explicit(value: string): SettingValue {
  return { mode: "explicit", value };
}

function textValue(value: SettingValue): string {
  return value.mode === "explicit" && typeof value.value === "string" ? value.value : "";
}

function SubagentModelRow({ spec, value, editor, busy }: {
  spec: Extract<SettingSpec, { control: "model" }>;
  value: SettingValue;
  editor: CodexEditorState;
  busy: boolean;
}) {
  const { connection, draft, setDraft } = editor;
  const current = textValue(value);
  const custom = value.mode === "explicit";
  const setValue = (next: SettingValue) => setDraft((existing) => existing.parameters
    ? { ...existing, parameters: { settings: { ...existing.parameters.settings, [spec.key]: next } } }
    : existing);
  const defaultModel = draft.defaultModel.trim();
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

function SubagentSettingsGroup({ specs, editor, busy, onChange }: {
  specs: SettingSpec[];
  editor: CodexEditorState;
  busy: boolean;
  onChange: (key: string, value: SettingValue) => void;
}) {
  if (specs.length === 0 || !editor.draft.parameters) return null;
  const settings = editor.draft.parameters.settings;
  const model = specs.find((spec): spec is Extract<SettingSpec, { control: "model" }> => spec.control === "model");
  return (
    <section className="asb-toggle-group">
      <div className="asb-toggle-group-head">
        <h3 className="asb-section-title">子 agent</h3>
      </div>
      <p className="asb-field-help">默认模型与推理强度随此供应商保存，并在切换到它时应用。</p>
      {model && <SubagentModelRow spec={model} value={settings[model.key]}
        editor={editor} busy={busy} />}
      {specs.filter((spec) => spec.control !== "model").map((spec) => (
        <SettingsRow key={spec.key} spec={spec} value={settings[spec.key]} showActual={false} busy={busy}
          onChange={(value) => onChange(spec.key, value)} />
      ))}
    </section>
  );
}

export function CodexParametersPage({ editor, busy }: Props) {
  const { draft, setDraft, parameters } = editor;
  const catalog = parameters.catalog;
  if (!catalog || !draft.parameters) return <ParametersLoadStatus busy={busy}
    ready={parameters.ready} error={parameters.error} retry={parameters.retry} />;
  const resetAll = () => setDraft((current) => current.parameters
    ? { ...current, parameters: { settings: { ...catalog.defaults.settings } } }
    : current);
  const subagentSpecs = catalog.specs.filter((spec) => spec.group === "子 agent");
  const parameterSpecs = catalog.specs.filter((spec) => spec.group !== "子 agent");
  const parameterGroups = catalog.groups.filter((group) => group !== "子 agent");
  const change = (key: string, value: SettingValue) => setDraft((current) => current.parameters
    ? { ...current, parameters: { settings: { ...current.parameters.settings, [key]: value } } }
    : current);
  return (
    <div className="asb-form" aria-label="供应商运行参数">
      <p className="asb-field-help">运行参数随此供应商保存，返回编辑后统一保存供应商。模型的上下文窗口在「模型」分区的目录行中配置。</p>
      <SettingsFields specs={parameterSpecs} groups={parameterGroups} values={draft.parameters.settings} busy={busy}
        showGroupReset={false} onChange={change} />
      <SubagentSettingsGroup specs={subagentSpecs} editor={editor} busy={busy} onChange={change} />
      <div className="asb-settings-actions">
        <Button variant="secondary" disabled={busy} onClick={resetAll}>恢复运行参数默认值</Button>
      </div>
    </div>
  );
}
