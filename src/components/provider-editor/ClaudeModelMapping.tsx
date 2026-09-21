import { useState, type Dispatch, type SetStateAction } from "react";
import type { ClaudeModelSettings, ProviderModel } from "../../api/client";
import { Checkbox } from "../Checkbox";
import { ChevronDownIcon } from "../icons";
import { Input } from "../Input";
import { ModelPicker } from "../ModelPicker";
import { Textarea } from "../Textarea";
import { claudeOptions, optional, type ProviderEditorDraft } from "./draft";

interface Props {
  busy: boolean;
  models: ProviderModel[] | null;
  claudeSettings: ClaudeModelSettings | null;
  setDraft: Dispatch<SetStateAction<ProviderEditorDraft>>;
}

interface TierProps extends Props {
  label: string;
  field: "haikuModel" | "sonnetModel" | "opusModel";
  contextFlag?: "sonnetOneM" | "opusOneM";
}

function ModelTierField({ label, field, contextFlag, busy, models, claudeSettings, setDraft }: TierProps) {
  const currentModel = claudeSettings?.[field] ?? null;
  const setModel = (model: string | null) => setDraft((current) => ({
    ...current,
    modelOptions: claudeOptions(current.modelOptions, {
      [field]: model,
      ...(!model && contextFlag ? { [contextFlag]: false } : {}),
    }),
  }));
  return (
    <div className="asb-field">
      <span>{label}</span>
      <div className="asb-input-with-picker">
        <Input code aria-label={label} value={currentModel ?? ""} disabled={busy}
          onChange={(event) => setModel(optional(event.target.value))} />
        {models && <ModelPicker models={models.map(({ id, ownedBy }) => ({ value: id, label: id, group: ownedBy }))} current={currentModel} ariaLabel={`选择 ${label}模型`}
          disabled={busy} onSelect={setModel} />}
        {contextFlag && (
          <Checkbox label="1M" ariaLabel={`${label}启用 1M 上下文`}
            checked={claudeSettings?.[contextFlag] ?? false} disabled={busy || !currentModel?.trim()}
            onChange={(enabled) => setDraft((current) => ({
              ...current, modelOptions: claudeOptions(current.modelOptions, { [contextFlag]: enabled }),
            }))} />
        )}
      </div>
    </div>
  );
}

function AvailableModelsField({ claudeSettings, busy, setDraft }: Props) {
  return (
    <label className="asb-field">
      <span>可选模型列表（每行一个）</span>
      <Textarea code rows={3} value={(claudeSettings?.availableModels ?? []).join("\n")} disabled={busy}
        onChange={(event) => {
          const lines = event.target.value.split("\n").map((line) => line.trim()).filter(Boolean);
          setDraft((current) => ({ ...current,
            modelOptions: claudeOptions(current.modelOptions, { availableModels: lines.length > 0 ? lines : null }),
          }));
        }} />
    </label>
  );
}

/** Claude tiers keep their model-specific context flags in the connection draft. */
export function ClaudeModelMapping(props: Props) {
  const values = props.claudeSettings;
  const hasMapping = Boolean(values && (values.haikuModel || values.sonnetModel || values.opusModel || values.availableModels?.length));
  const [expanded, setExpanded] = useState(hasMapping);
  return (
    <details className="asb-provider-disclosure" open={expanded}
      onToggle={(event) => setExpanded(event.currentTarget.open)}>
      <summary><span>模型映射</span><span className="asb-provider-disclosure-value">{hasMapping ? "已配置" : "按档位覆盖"}</span><ChevronDownIcon /></summary>
      <div className="asb-provider-disclosure-body asb-provider-field-grid">
        <ModelTierField {...props} label="Haiku 档" field="haikuModel" />
        <ModelTierField {...props} label="Sonnet 档" field="sonnetModel" contextFlag="sonnetOneM" />
        <ModelTierField {...props} label="Opus 档" field="opusModel" contextFlag="opusOneM" />
        <AvailableModelsField {...props} />
      </div>
    </details>
  );
}
