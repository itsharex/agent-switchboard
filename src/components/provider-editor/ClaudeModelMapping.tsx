import type { Dispatch, SetStateAction } from "react";
import type {
  ClaudeModelSettings,
  ProviderDraft,
  ProviderModel,
} from "../../api/client";
import { Checkbox } from "../Checkbox";
import { Input } from "../Input";
import { ModelPicker } from "../ModelPicker";
import { Textarea } from "../Textarea";
import { claudeOptions, optional } from "./draft";

interface Props {
  busy: boolean;
  models: ProviderModel[] | null;
  claudeSettings: ClaudeModelSettings | null;
  setDraft: Dispatch<SetStateAction<ProviderDraft>>;
}

/** Claude tier model mapping (Haiku / Sonnet / Opus) and the pickable list. */
export function ClaudeModelMapping({ busy, models, claudeSettings, setDraft }: Props) {
  return (
    <fieldset className="asb-fieldset">
      <legend>模型映射</legend>
      <div className="asb-field">
        <span>Haiku 档</span>
        <div className="asb-input-with-picker">
          <Input
            code
            aria-label="Haiku 档"
            value={claudeSettings?.haikuModel ?? ""}
            disabled={busy}
            onChange={(event) =>
              setDraft((current) => ({
                ...current,
                modelOptions: claudeOptions(current.modelOptions, {
                  haikuModel: optional(event.target.value),
                }),
              }))
            }
          />
          {models && (
            <ModelPicker
              models={models}
              current={claudeSettings?.haikuModel ?? null}
              ariaLabel="选择 Haiku 档模型"
              disabled={busy}
              onSelect={(haikuModel) =>
                setDraft((current) => ({
                  ...current,
                  modelOptions: claudeOptions(current.modelOptions, { haikuModel }),
                }))
              }
            />
          )}
        </div>
      </div>
      <div className="asb-field">
        <span>Sonnet 档</span>
        <div className="asb-input-with-picker">
          <Input
            code
            aria-label="Sonnet 档"
            value={claudeSettings?.sonnetModel ?? ""}
            disabled={busy}
            onChange={(event) =>
              setDraft((current) => {
                const sonnetModel = optional(event.target.value);
                return {
                  ...current,
                  modelOptions: claudeOptions(current.modelOptions, {
                    sonnetModel,
                    ...(sonnetModel ? {} : { sonnetOneM: false }),
                  }),
                };
              })
            }
          />
          {models && (
            <ModelPicker
              models={models}
              current={claudeSettings?.sonnetModel ?? null}
              ariaLabel="选择 Sonnet 档模型"
              disabled={busy}
              onSelect={(sonnetModel) =>
                setDraft((current) => ({
                  ...current,
                  modelOptions: claudeOptions(current.modelOptions, { sonnetModel }),
                }))
              }
            />
          )}
          <Checkbox
            label="1M"
            ariaLabel="Sonnet 档启用 1M 上下文"
            checked={claudeSettings?.sonnetOneM ?? false}
            disabled={busy || !claudeSettings?.sonnetModel?.trim()}
            onChange={(enabled) =>
              setDraft((current) => ({
                ...current,
                modelOptions: claudeOptions(current.modelOptions, { sonnetOneM: enabled }),
              }))
            }
          />
        </div>
      </div>
      <div className="asb-field">
        <span>Opus 档</span>
        <div className="asb-input-with-picker">
          <Input
            code
            aria-label="Opus 档"
            value={claudeSettings?.opusModel ?? ""}
            disabled={busy}
            onChange={(event) =>
              setDraft((current) => {
                const opusModel = optional(event.target.value);
                return {
                  ...current,
                  modelOptions: claudeOptions(current.modelOptions, {
                    opusModel,
                    ...(opusModel ? {} : { opusOneM: false }),
                  }),
                };
              })
            }
          />
          {models && (
            <ModelPicker
              models={models}
              current={claudeSettings?.opusModel ?? null}
              ariaLabel="选择 Opus 档模型"
              disabled={busy}
              onSelect={(opusModel) =>
                setDraft((current) => ({
                  ...current,
                  modelOptions: claudeOptions(current.modelOptions, { opusModel }),
                }))
              }
            />
          )}
          <Checkbox
            label="1M"
            ariaLabel="Opus 档启用 1M 上下文"
            checked={claudeSettings?.opusOneM ?? false}
            disabled={busy || !claudeSettings?.opusModel?.trim()}
            onChange={(enabled) =>
              setDraft((current) => ({
                ...current,
                modelOptions: claudeOptions(current.modelOptions, { opusOneM: enabled }),
              }))
            }
          />
        </div>
      </div>
      <label className="asb-field">
        <span>可选模型列表（每行一个）</span>
        <Textarea
          code
          rows={3}
          value={(claudeSettings?.availableModels ?? []).join("\n")}
          disabled={busy}
          onChange={(event) =>
            setDraft((current) => {
              const lines = event.target.value
                .split("\n")
                .map((line) => line.trim())
                .filter(Boolean);
              return {
                ...current,
                modelOptions: claudeOptions(current.modelOptions, {
                  availableModels: lines.length > 0 ? lines : null,
                }),
              };
            })
          }
        />
      </label>
    </fieldset>
  );
}
