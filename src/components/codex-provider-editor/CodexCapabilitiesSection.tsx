import { useState } from "react";
import type { CodexCapabilities, CodexChatReasoning } from "../../api/client";
import { Checkbox } from "../Checkbox";
import { ChevronDownIcon } from "../icons";
import { RadioOption } from "../RadioOption";
import { Select } from "../Select";
import { reconcileCodexCapabilities } from "./draft";
import type { CodexEditorState } from "./useCodexProviderEditor";

type ConfiguredChatReasoning = Extract<CodexChatReasoning, { kind: "configured" }>;

interface Props { editor: CodexEditorState; busy: boolean }

const CAPABILITY_LABELS: Array<{ key: keyof Omit<CodexCapabilities, "chatReasoning">; label: string; help?: string }> = [
  { key: "compact", label: "可压缩" },
  { key: "models", label: "模型操作" },
  { key: "chatCompletions", label: "Chat 兼容" },
  { key: "alphaSearch", label: "网页搜索（alpha）" },
  { key: "imageGeneration", label: "图像生成" },
  { key: "imageEdit", label: "图像编辑" },
  { key: "functionTools", label: "函数工具" },
  { key: "customTools", label: "自定义工具" },
  { key: "toolSearch", label: "工具搜索" },
  { key: "reasoning", label: "推理" },
];

function ChatReasoningControls({ editor, busy }: Props) {
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
      <span>Chat 推理参数</span>
      <div className="asb-segments" role="radiogroup" aria-label="Chat 推理参数">
        <RadioOption name="chat-reasoning" checked={!configured} disabled={busy} label="不使用"
          onChange={() => setDraft((current) => reconcileCodexCapabilities(current,
            { ...current.capabilities, chatReasoning: { kind: "unsupported" } }))} />
        <RadioOption name="chat-reasoning" checked={configured} disabled={busy} label="配置"
          onChange={() => setDraft((current) => ({ ...current, capabilities: { ...current.capabilities,
            chatReasoning: { kind: "configured", thinkingParameter: "none", effortParameter: "none", effortMode: "passthrough" } } }))} />
      </div>
      {chatReasoning.kind === "configured" && (
        <div className="asb-provider-field-grid asb-provider-chat-reasoning">
          <label className="asb-field">
            <span>思考开关参数</span>
            <Select ariaLabel="思考开关参数" value={chatReasoning.thinkingParameter} disabled={busy}
              options={[
                { value: "none", label: "不映射" },
                { value: "thinking", label: "thinking" },
                { value: "enableThinking", label: "enable_thinking" },
                { value: "reasoningSplit", label: "推理拆分" },
              ]}
              onChange={(value) => patch({ thinkingParameter: value as ConfiguredChatReasoning["thinkingParameter"] })} />
          </label>
          <label className="asb-field">
            <span>推理力度参数</span>
            <Select ariaLabel="推理力度参数" value={chatReasoning.effortParameter} disabled={busy}
              options={[
                { value: "none", label: "不映射" },
                { value: "reasoningEffort", label: "reasoning_effort" },
                { value: "reasoningObject", label: "推理对象" },
              ]}
              onChange={(value) => patch({ effortParameter: value as ConfiguredChatReasoning["effortParameter"] })} />
          </label>
          <label className="asb-field">
            <span>力度映射</span>
            <Select ariaLabel="力度映射" value={chatReasoning.effortMode} disabled={busy}
              options={[
                { value: "passthrough", label: "直通" },
                { value: "lowHigh", label: "低-高" },
                { value: "deepSeek", label: "DeepSeek" },
                { value: "openRouter", label: "OpenRouter" },
                { value: "catalog", label: "按模型目录档位" },
              ]}
              onChange={(value) => patch({ effortMode: value as ConfiguredChatReasoning["effortMode"] })} />
          </label>
        </div>
      )}
      <p className="asb-scope-note">仅 Chat Completions 上游且启用推理时可用；描述上游如何接收 Codex 的推理档位。</p>
    </div>
  );
}

/** Explicit capability declaration; entries generated from /models only ever
 * narrow against what is declared here. */
export function CodexCapabilitiesSection({ editor, busy }: Props) {
  const { draft, setDraft } = editor;
  const [expanded, setExpanded] = useState(false);
  const enabledCount = CAPABILITY_LABELS.filter(({ key }) => draft.capabilities[key]).length;
  return (
    <details className="asb-provider-disclosure" open={expanded}
      onToggle={(event) => setExpanded(event.currentTarget.open)}>
      <summary><span>供应商能力</span><span className="asb-provider-disclosure-value">
        {enabledCount} 项已启用
      </span><ChevronDownIcon /></summary>
      <div className="asb-provider-disclosure-body">
        <p className="asb-scope-note">能力按操作逐项声明，不会被模型列表推断；关闭某项能力会同步收窄模型目录。</p>
        <div className="asb-provider-field-grid">
          <div className="asb-field">
            <span>基础能力</span>
            <div className="asb-catalog-levels-options" role="group" aria-label="基础能力">
              <Checkbox label="Responses" checked disabled
                onChange={() => undefined} />
            </div>
            <p className="asb-scope-note">固定开启：Codex 客户端只说 Responses 协议。</p>
          </div>
          <div className="asb-field">
            <span>操作能力</span>
            <div className="asb-catalog-levels-options" role="group" aria-label="操作能力">
              {CAPABILITY_LABELS.map(({ key, label }) => (
                <Checkbox key={key} label={label} checked={draft.capabilities[key]} disabled={busy}
                  onChange={(checked) => setDraft((current) => reconcileCodexCapabilities(current,
                    { ...current.capabilities, [key]: checked }))} />
              ))}
            </div>
          </div>
        </div>
        {draft.upstream === "chatCompletions" && draft.capabilities.reasoning
          ? <ChatReasoningControls editor={editor} busy={busy} />
          : <p className="asb-scope-note">Chat 推理参数仅 Chat Completions 上游且启用推理时可配置。</p>}
      </div>
    </details>
  );
}
