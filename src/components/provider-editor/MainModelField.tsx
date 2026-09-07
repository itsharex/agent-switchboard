import type { Dispatch, SetStateAction } from "react";
import type {
  ClaudeModelSettings,
  CodexModelSettings,
  ProviderDraft,
  ProviderModel,
} from "../../api/client";
import { Button } from "../Button";
import { Checkbox } from "../Checkbox";
import { Input } from "../Input";
import { ModelPicker } from "../ModelPicker";
import { ProbePanel } from "../ProbePanel";
import { CONTEXT_WINDOW_1M, claudeOptions, codexOptions } from "./draft";

interface Props {
  draft: ProviderDraft;
  busy: boolean;
  baseUrl: string;
  codex: boolean;
  codexSettings: CodexModelSettings | null;
  claudeSettings: ClaudeModelSettings | null;
  models: ProviderModel[] | null;
  modelsBusy: boolean;
  modelsError: string | null;
  userConfigModel: string | null;
  userConfigWarnings: string[];
  fetchModels: () => void | Promise<void>;
  setDraft: Dispatch<SetStateAction<ProviderDraft>>;
}

/** Main model field with the quick picker, 1M flag, probe, and model fetch. */
export function MainModelField({
  draft,
  busy,
  baseUrl,
  codex,
  codexSettings,
  claudeSettings,
  models,
  modelsBusy,
  modelsError,
  userConfigModel,
  userConfigWarnings,
  fetchModels,
  setDraft,
}: Props) {
  return (
    <div className="asb-field">
      <span>主模型</span>
      <div className="asb-model-control">
        <Input
          aria-label="主模型"
          value={draft.model ?? ""}
          disabled={busy}
          placeholder="（可选）"
          onChange={(event) =>
            setDraft((current) => {
              const model = event.target.value;
              if (
                !codex &&
                !model.trim() &&
                current.modelOptions?.kind === "claude"
              ) {
                return {
                  ...current,
                  model,
                  modelOptions: { ...current.modelOptions, primaryOneM: false },
                };
              }
              return { ...current, model };
            })
          }
        />
        {models && (
          <ModelPicker
            models={models}
            current={draft.model}
            ariaLabel="选择模型"
            disabled={busy}
            onSelect={(model) => setDraft((current) => ({ ...current, model }))}
          />
        )}
        {codex ? (
          <Checkbox
            label="1M"
            ariaLabel="启用 1M 上下文窗口"
            checked={codexSettings?.contextWindow === CONTEXT_WINDOW_1M}
            disabled={busy}
            onChange={(checked) =>
              setDraft((current) => ({
                ...current,
                modelOptions: codexOptions(current.modelOptions, {
                  contextWindow: checked ? CONTEXT_WINDOW_1M : null,
                }),
              }))
            }
          />
        ) : (
          <Checkbox
            label="1M"
            ariaLabel="主模型启用 1M 上下文"
            checked={claudeSettings?.primaryOneM ?? false}
            disabled={busy || !draft.model?.trim()}
            onChange={(enabled) =>
              setDraft((current) => ({
                ...current,
                modelOptions: claudeOptions(current.modelOptions, { primaryOneM: enabled }),
              }))
            }
          />
        )}
        <div className="asb-model-actions">
          <ProbePanel url={draft.baseUrl?.trim() || null} />
          <Button
            variant="secondary"
            disabled={busy || modelsBusy || !baseUrl}
            onClick={() => void fetchModels()}
          >
            {modelsBusy ? "获取中…" : "获取模型"}
          </Button>
        </div>
      </div>
      {userConfigModel && (
        <p className="asb-scope-note">当前用户级配置模型：{userConfigModel}</p>
      )}
      {userConfigWarnings.map((warning) => (
        <p key={warning} className="asb-scope-note asb-warn-text">
          {warning}
        </p>
      ))}
      {modelsError && <span className="asb-warn-text">{modelsError}</span>}
    </div>
  );
}
