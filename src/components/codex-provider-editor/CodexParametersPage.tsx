import type { SettingSpec, SettingValue } from "../../api/client";
import { Button } from "../Button";
import { Input } from "../Input";
import { ModelPicker } from "../ModelPicker";
import { RadioOption } from "../RadioOption";
import { choiceLabel, sameSettingValue, SettingsFields, SettingsRow } from "../SettingsFields";
import { ParametersLoadStatus } from "../provider-editor/ProviderParametersPage";
import type { CodexEditorState } from "./useCodexProviderEditor";

interface Props {
  editor: CodexEditorState;
  busy: boolean;
  baselineValues?: Record<string, SettingValue>;
}

const automatic: SettingValue = { mode: "automatic" };

function explicit(value: string): SettingValue {
  return { mode: "explicit", value };
}

function textValue(value: SettingValue): string {
  return value.mode === "explicit" && typeof value.value === "string" ? value.value : "";
}

function SubagentModelRow({ spec, value, baselineValue, editor, busy }: {
  spec: Extract<SettingSpec, { control: "model" }>;
  value: SettingValue;
  baselineValue: SettingValue;
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
  const changed = !sameSettingValue(baselineValue, value);
  return (
    <div className="asb-toggle-row asb-choice-row">
      <div className="asb-choice-head">
        <div className="asb-app-setting-copy">
          <span className="asb-checkbox-label">{spec.label}</span>
          <span className="asb-app-setting-detail">自动时使用供应商默认模型；指定后随供应商一起保存，任务或角色显式模型仍可覆盖。</span>
        </div>
        <span className="asb-setting-actual" aria-live="polite">当前设置：{choiceLabel(spec, baselineValue)}</span>
      </div>
      <div className="asb-choice-controls">
        <div className="asb-segments" role="radiogroup" aria-label={`${spec.label}配置方式`}>
          <RadioOption name={`${spec.key}-mode`} checked={!custom} disabled={busy} label="自动"
            onChange={() => setValue(automatic)} />
          <RadioOption name={`${spec.key}-mode`} checked={custom} disabled={busy} label="指定"
            onChange={() => setValue(explicit(current || defaultModel))} />
        </div>
        {custom && (
          <div className="asb-model-control asb-provider-subagent-model-control">
            <Input code aria-label={`${spec.label}值`} autoComplete="off" disabled={busy}
              value={current} onChange={(event) => setValue(explicit(event.target.value))} />
            {connection.models && (
              <ModelPicker models={connection.models} current={current || null}
                ariaLabel={`选择${spec.label}`} disabled={busy || connection.modelsBusy}
                onSelect={(model) => setValue(explicit(model))} />
            )}
            <Button variant="secondary" disabled={busy || connection.modelsBusy || !connection.baseUrl || !!connection.modelsEndpointError}
              onClick={() => void connection.fetchModels()}>
              {connection.modelsBusy ? "获取中…" : "获取模型"}
            </Button>
            {connection.modelsEndpointError && <span className="asb-warn-text">{connection.modelsEndpointError}</span>}
            {connection.modelsError && connection.modelsError !== connection.modelsEndpointError && <span className="asb-warn-text">{connection.modelsError}</span>}
          </div>
        )}
      </div>
      {changed && (
        <p className="asb-setting-pending" role="status">
          {value.mode === "automatic" ? "待保存：移除此项，恢复默认模型" : `待保存：写入「${choiceLabel(spec, value)}」`}
        </p>
      )}
    </div>
  );
}

function SubagentSettingsGroup({ specs, baselineValues, editor, busy, onChange }: {
  specs: SettingSpec[];
  baselineValues: Record<string, SettingValue>;
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
        baselineValue={baselineValues[model.key] ?? automatic} editor={editor} busy={busy} />}
      {specs.filter((spec) => spec.control !== "model").map((spec) => (
        <SettingsRow key={spec.key} spec={spec} value={settings[spec.key]}
          baselineValue={baselineValues[spec.key] ?? automatic} showActual={false} busy={busy}
          presentation="provider" onChange={(value) => onChange(spec.key, value)} />
      ))}
    </section>
  );
}

export function CodexParametersPage({ editor, busy, baselineValues }: Props) {
  const { draft, setDraft, parameters } = editor;
  const catalog = parameters.catalog;
  if (!catalog || !draft.parameters) return <ParametersLoadStatus busy={busy}
    ready={parameters.ready} error={parameters.error} retry={parameters.retry} />;
  const resetAll = () => setDraft((current) => current.parameters
    ? { ...current, parameters: { settings: { ...catalog.defaults.settings } } }
    : current);
  const baseline = baselineValues ?? catalog.defaults.settings;
  const subagentSpecs = catalog.specs.filter((spec) => spec.group === "子 agent");
  const parameterSpecs = catalog.specs.filter((spec) => spec.group !== "子 agent");
  const parameterGroups = catalog.groups.filter((group) => group !== "子 agent");
  const change = (key: string, value: SettingValue) => setDraft((current) => current.parameters
    ? { ...current, parameters: { settings: { ...current.parameters.settings, [key]: value } } }
    : current);
  return (
    <div className="asb-form asb-provider-parameters" aria-label="供应商运行参数">
      <p className="asb-field-help">运行参数随此供应商保存；当前设置来自已保存的供应商档案，新建供应商从默认值开始。模型的上下文窗口在「模型」分区的目录行中配置。</p>
      <SettingsFields specs={parameterSpecs} groups={parameterGroups} values={draft.parameters.settings}
        baselineValues={baseline} busy={busy} showGroupReset={false} presentation="provider" onChange={change} />
      <SubagentSettingsGroup specs={subagentSpecs} baselineValues={baseline}
        editor={editor} busy={busy} onChange={change} />
      <div className="asb-settings-actions">
        <Button variant="secondary" disabled={busy} onClick={resetAll}>恢复运行参数默认值</Button>
      </div>
    </div>
  );
}
