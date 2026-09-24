import { useState } from "react";
import type { CodexCapabilities, CodexChatReasoning } from "../../api/client";
import type { MessageKey } from "../../i18n";
import { useI18n } from "../../i18n";
import { Checkbox } from "../Checkbox";
import { ChevronDownIcon } from "../icons";
import { RadioOption } from "../RadioOption";
import { Select } from "../Select";
import { reconcileCodexCapabilities } from "./draft";
import type { CodexEditorState } from "./useCodexProviderEditor";

type ConfiguredChatReasoning = Extract<CodexChatReasoning, { kind: "configured" }>;

interface Props { editor: CodexEditorState; busy: boolean }

const CAPABILITY_LABELS: Array<{ key: keyof Omit<CodexCapabilities, "chatReasoning">; labelKey: MessageKey }> = [
  { key: "compact", labelKey: "codex.capability.compact" },
  { key: "models", labelKey: "codex.capability.models" },
  { key: "chatCompletions", labelKey: "codex.capability.chatCompletions" },
  { key: "alphaSearch", labelKey: "codex.capability.alphaSearch" },
  { key: "imageGeneration", labelKey: "codex.capability.imageGeneration" },
  { key: "imageEdit", labelKey: "codex.capability.imageEdit" },
  { key: "functionTools", labelKey: "codex.capability.functionTools" },
  { key: "customTools", labelKey: "codex.capability.customTools" },
  { key: "toolSearch", labelKey: "codex.capability.toolSearch" },
  { key: "reasoning", labelKey: "codex.capability.reasoning" },
];

function ChatReasoningControls({ editor, busy }: Props) {
  const { t } = useI18n();
  const { draft, setDraft } = editor;
  const chatReasoning = draft.capabilities.chatReasoning;
  const configured = chatReasoning.kind === "configured";
  const patch = (patchFields: Partial<ConfiguredChatReasoning>) =>
    setDraft((current) => current.capabilities.chatReasoning.kind === "configured"
      ? { ...current, capabilities: { ...current.capabilities,
          chatReasoning: { ...current.capabilities.chatReasoning, ...patchFields } } }
      : current);
  return (
    <div className="asb-field">
      <span>{t("codex.chatReasoning.title")}</span>
      <div className="asb-segments" role="radiogroup" aria-label={t("codex.chatReasoning.title")}>
        <RadioOption name="chat-reasoning" checked={!configured} disabled={busy} label={t("codex.chatReasoning.unused")}
          onChange={() => setDraft((current) => reconcileCodexCapabilities(current,
            { ...current.capabilities, chatReasoning: { kind: "unsupported" } }))} />
        <RadioOption name="chat-reasoning" checked={configured} disabled={busy} label={t("codex.chatReasoning.configure")}
          onChange={() => setDraft((current) => ({ ...current, capabilities: { ...current.capabilities,
            chatReasoning: { kind: "configured", thinkingParameter: "none", effortParameter: "none", effortMode: "passthrough" } } }))} />
      </div>
      {chatReasoning.kind === "configured" && (
        <div className="asb-provider-field-grid asb-provider-chat-reasoning">
          <label className="asb-field">
            <span>{t("codex.chatReasoning.thinkingParam")}</span>
            <Select ariaLabel={t("codex.chatReasoning.thinkingParam")} value={chatReasoning.thinkingParameter} disabled={busy}
              options={[
                { value: "none", label: t("codex.chatReasoning.noMapping") },
                { value: "thinking", label: "thinking" },
                { value: "enableThinking", label: "enable_thinking" },
                { value: "reasoningSplit", label: t("codex.chatReasoning.reasoningSplit") },
              ]}
              onChange={(value) => patch({ thinkingParameter: value as ConfiguredChatReasoning["thinkingParameter"] })} />
          </label>
          <label className="asb-field">
            <span>{t("codex.chatReasoning.effortParam")}</span>
            <Select ariaLabel={t("codex.chatReasoning.effortParam")} value={chatReasoning.effortParameter} disabled={busy}
              options={[
                { value: "none", label: t("codex.chatReasoning.noMapping") },
                { value: "reasoningEffort", label: "reasoning_effort" },
                { value: "reasoningObject", label: t("codex.chatReasoning.reasoningObject") },
              ]}
              onChange={(value) => patch({ effortParameter: value as ConfiguredChatReasoning["effortParameter"] })} />
          </label>
          <label className="asb-field">
            <span>{t("codex.chatReasoning.effortMode")}</span>
            <Select ariaLabel={t("codex.chatReasoning.effortMode")} value={chatReasoning.effortMode} disabled={busy}
              options={[
                { value: "passthrough", label: t("codex.chatReasoning.passthrough") },
                { value: "lowHigh", label: t("codex.chatReasoning.lowHigh") },
                { value: "deepSeek", label: "DeepSeek" },
                { value: "openRouter", label: "OpenRouter" },
                { value: "catalog", label: t("codex.chatReasoning.catalogLevels") },
              ]}
              onChange={(value) => patch({ effortMode: value as ConfiguredChatReasoning["effortMode"] })} />
          </label>
        </div>
      )}
      <p className="asb-scope-note">{t("codex.chatReasoning.note")}</p>
    </div>
  );
}

/** Explicit capability declaration; entries generated from /models only ever
 * narrow against what is declared here. */
export function CodexCapabilitiesSection({ editor, busy }: Props) {
  const { t } = useI18n();
  const { draft, setDraft } = editor;
  const [expanded, setExpanded] = useState(false);
  const enabledCount = CAPABILITY_LABELS.filter(({ key }) => draft.capabilities[key]).length;
  return (
    <details className="asb-provider-disclosure" open={expanded}
      onToggle={(event) => setExpanded(event.currentTarget.open)}>
      <summary><span>{t("codex.capabilities.title")}</span><span className="asb-provider-disclosure-value">
        {t("codex.capabilities.enabledCount", { count: enabledCount })}
      </span><ChevronDownIcon /></summary>
      <div className="asb-provider-disclosure-body">
        <p className="asb-scope-note">{t("codex.capabilities.note")}</p>
        <div className="asb-provider-field-grid">
          <div className="asb-field">
            <span>{t("codex.capabilities.basic")}</span>
            <div className="asb-catalog-levels-options" role="group" aria-label={t("codex.capabilities.basic")}>
              <Checkbox label="Responses" checked disabled
                onChange={() => undefined} />
            </div>
            <p className="asb-scope-note">{t("codex.capabilities.alwaysOn")}</p>
          </div>
          <div className="asb-field">
            <span>{t("codex.capabilities.operations")}</span>
            <div className="asb-catalog-levels-options" role="group" aria-label={t("codex.capabilities.operations")}>
              {CAPABILITY_LABELS.map(({ key, labelKey }) => (
                <Checkbox key={key} label={t(labelKey)} checked={draft.capabilities[key]} disabled={busy}
                  onChange={(checked) => setDraft((current) => reconcileCodexCapabilities(current,
                    { ...current.capabilities, [key]: checked }))} />
              ))}
            </div>
          </div>
        </div>
        {draft.upstream === "chatCompletions" && draft.capabilities.reasoning
          ? <ChatReasoningControls editor={editor} busy={busy} />
          : <p className="asb-scope-note">{t("codex.capabilities.chatReasoningOffNote")}</p>}
      </div>
    </details>
  );
}
