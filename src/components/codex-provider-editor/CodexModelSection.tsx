import { useState } from "react";
import type { CodexReasoningLevel } from "../../api/client";
import { Button } from "../Button";
import { Checkbox } from "../Checkbox";
import { ChevronDownIcon, PlusIcon, TrashIcon } from "../icons";
import { Input } from "../Input";
import { Select } from "../Select";
import { Tooltip } from "../Tooltip";
import { UpdateIcon } from "../icons";
import {
  REASONING_LEVELS,
  REASONING_LEVEL_LABELS,
  defaultModelLimits,
  emptyCatalogEntry,
  mergeFetchedCatalog,
} from "./draft";
import type { CodexEditorState } from "./useCodexProviderEditor";
import type { EditableCodexCatalogEntry } from "./draft";

interface Props {
  editor: CodexEditorState;
  busy: boolean;
  userConfigModel: string | null;
  userConfigWarnings: string[];
}

/** Parses a limit input: a positive integer is stored as-is, anything else
 * (including empty) means "use the model's default". */
function parseLimitInput(raw: string): number | null {
  const parsed = Number(raw.trim());
  return raw.trim() && Number.isSafeInteger(parsed) && parsed > 0 ? parsed : null;
}

function CatalogRow({ editor, busy, entry, index }: {
  editor: CodexEditorState;
  busy: boolean;
  entry: EditableCodexCatalogEntry;
  index: number;
}) {
  const { draft, setDraft } = editor;
  const { capabilities } = draft;
  const defaults = defaultModelLimits(entry.id);
  const update = (patch: Partial<EditableCodexCatalogEntry>) =>
    setDraft((current) => ({
      ...current,
      catalog: current.catalog.map((item, itemIndex) =>
        itemIndex === index ? { ...item, ...patch } : item),
    }));
  const toggleLevel = (level: CodexReasoningLevel) => {
    const levels = entry.supportedReasoningLevels.includes(level)
      ? entry.supportedReasoningLevels.filter((value) => value !== level)
      : [...entry.supportedReasoningLevels, level];
    const ordered = REASONING_LEVELS.filter((value) => levels.includes(value));
    const defaultLevel = ordered.includes(entry.defaultReasoningLevel)
      ? entry.defaultReasoningLevel
      : ordered[0] ?? "none";
    update({ supportedReasoningLevels: ordered, defaultReasoningLevel: defaultLevel });
  };
  const toggleReasoning = (reasoning: boolean) => update(reasoning
    ? { reasoning, defaultReasoningLevel: "medium", supportedReasoningLevels: [...REASONING_LEVELS] }
    : { reasoning, defaultReasoningLevel: "none", supportedReasoningLevels: ["none"] });
  return (
    <div className="asb-provider-catalog-row" aria-label={`模型 ${entry.id || "未命名"}`}>
      <div className="asb-provider-catalog-row-head">
        <label className="asb-field">
          <span>模型标识</span>
          <Input code aria-label={`模型标识 ${index + 1}`} value={entry.id} required disabled={busy}
            onChange={(event) => update({ id: event.target.value })} />
        </label>
        <Tooltip label={`删除模型 ${entry.id || index + 1}`}>
          <Button variant="icon" aria-label={`删除模型 ${entry.id || index + 1}`} disabled={busy}
            onClick={() => setDraft((current) => ({
              ...current,
              catalog: current.catalog.filter((_, itemIndex) => itemIndex !== index),
              modelRoutes: current.modelRoutes.filter((route) => route.clientModel !== entry.id),
              defaultModel: current.defaultModel === entry.id ? "" : current.defaultModel,
            }))}>
            <TrashIcon />
          </Button>
        </Tooltip>
      </div>
      <div className="asb-provider-field-grid">
        <label className="asb-field">
          <span>上下文窗口</span>
          <Input aria-label={`${entry.id || "模型"} 上下文窗口`} type="number" min="1" step="1"
            placeholder={`默认 ${defaults.contextWindow.toLocaleString("en-US")}`}
            value={entry.contextWindow?.toString() ?? ""} disabled={busy}
            onChange={(event) => update({ contextWindow: parseLimitInput(event.target.value) })} />
        </label>
        <label className="asb-field">
          <span>输出上限</span>
          <Input aria-label={`${entry.id || "模型"} 输出上限`} type="number" min="1" step="1"
            placeholder={`默认 ${defaults.maxOutputTokens.toLocaleString("en-US")}`}
            value={entry.maxOutputTokens?.toString() ?? ""} disabled={busy}
            onChange={(event) => update({ maxOutputTokens: parseLimitInput(event.target.value) })} />
        </label>
        <label className="asb-field">
          <span>默认推理档位</span>
          <Select ariaLabel={`${entry.id || "模型"} 默认推理档位`} disabled={busy}
            value={entry.defaultReasoningLevel}
            options={entry.supportedReasoningLevels.map((level) => ({
              value: level, label: REASONING_LEVEL_LABELS[level],
            }))}
            onChange={(value) => update({ defaultReasoningLevel: value as CodexReasoningLevel })} />
        </label>
      </div>
      <div className="asb-provider-catalog-row-levels">
        <span className="asb-catalog-levels-label">支持推理档位</span>
        <div className="asb-catalog-levels-options" role="group" aria-label={`${entry.id || "模型"} 支持推理档位`}>
          {REASONING_LEVELS.map((level) => (
            <Checkbox key={level} label={REASONING_LEVEL_LABELS[level]}
              ariaLabel={`${entry.id || "模型"} 支持${REASONING_LEVEL_LABELS[level]}档位`}
              checked={entry.supportedReasoningLevels.includes(level)}
              disabled={busy || !entry.reasoning}
              onChange={() => toggleLevel(level)} />
          ))}
        </div>
      </div>
      <div className="asb-provider-catalog-row-flags" role="group" aria-label={`${entry.id || "模型"} 能力`}>
        <Checkbox label="函数工具" ariaLabel={`${entry.id || "模型"} 函数工具`} checked={entry.functionTools}
          disabled={busy || !capabilities.functionTools}
          onChange={(checked) => update({ functionTools: checked && capabilities.functionTools })} />
        <Checkbox label="自定义工具" ariaLabel={`${entry.id || "模型"} 自定义工具`} checked={entry.customTools}
          disabled={busy || !capabilities.customTools}
          onChange={(checked) => update({ customTools: checked && capabilities.customTools })} />
        <Checkbox label="工具搜索" ariaLabel={`${entry.id || "模型"} 工具搜索`} checked={entry.toolSearch}
          disabled={busy || !capabilities.toolSearch}
          onChange={(checked) => update({ toolSearch: checked && capabilities.toolSearch })} />
        <Checkbox label="推理" ariaLabel={`${entry.id || "模型"} 推理`} checked={entry.reasoning} disabled={busy || !capabilities.reasoning}
          onChange={(checked) => toggleReasoning(checked && capabilities.reasoning)} />
        <Checkbox label="图像" ariaLabel={`${entry.id || "模型"} 图像`} checked={entry.images} disabled={busy}
          onChange={(checked) => update({ images: checked })} />
        <Checkbox label="压缩" ariaLabel={`${entry.id || "模型"} 压缩`} checked={entry.compact}
          disabled={busy || !capabilities.compact}
          onChange={(checked) => update({ compact: checked && capabilities.compact })} />
      </div>
      {entry.reasoning && !capabilities.reasoning && (
        <p className="asb-scope-note asb-warn-text">供应商能力未声明推理。</p>
      )}
    </div>
  );
}

function ModelMapping({ editor, busy }: { editor: CodexEditorState; busy: boolean }) {
  const { draft, setDraft } = editor;
  const [expanded, setExpanded] = useState(() => draft.modelRoutes.length > 0);
  const update = (index: number, clientModel: string, upstreamModel: string) =>
    setDraft((current) => ({
      ...current,
      modelRoutes: current.modelRoutes.map((route, routeIndex) =>
        routeIndex === index ? { clientModel, upstreamModel } : route),
    }));
  return (
    <details className="asb-provider-disclosure" open={expanded}
      onToggle={(event) => setExpanded(event.currentTarget.open)}>
      <summary><span>模型映射</span><span className="asb-provider-disclosure-value">
        {draft.modelRoutes.length > 0 ? `${draft.modelRoutes.length} 条` : "未配置"}
      </span><ChevronDownIcon /></summary>
      <div className="asb-provider-disclosure-body">
        <p className="asb-scope-note">把客户端模型映射到上游的实际模型名；未映射的模型按名称直通（Responses）或使用默认模型。</p>
        <div className="asb-provider-mapping-list">
          {draft.modelRoutes.map((route, index) => (
            <div className="asb-provider-mapping-row" key={index} aria-label="模型映射">
              <Select ariaLabel={`映射客户端模型 ${index + 1}`} value={route.clientModel} disabled={busy}
                options={draft.catalog.map((entry) => ({ value: entry.id, label: entry.id }))}
                onChange={(value) => update(index, value, route.upstreamModel)} />
              <span className="asb-provider-mapping-arrow" aria-hidden="true">→</span>
              <Input code aria-label={`映射上游模型 ${index + 1}`} value={route.upstreamModel}
                disabled={busy} onChange={(event) => update(index, route.clientModel, event.target.value)} />
              <Button variant="danger" aria-label={`删除映射 ${index + 1}`} disabled={busy}
                onClick={() => setDraft((current) => ({
                  ...current,
                  modelRoutes: current.modelRoutes.filter((_, routeIndex) => routeIndex !== index),
                }))}>
                <TrashIcon />
              </Button>
            </div>
          ))}
        </div>
        <div className="asb-provider-model-actions asb-provider-model-actions-end">
          <Tooltip label="添加映射">
            <Button variant="icon" aria-label="添加映射" disabled={busy || draft.catalog.length === 0}
              onClick={() => setDraft((current) => ({
                ...current,
                modelRoutes: [...current.modelRoutes,
                  { clientModel: current.catalog[0]?.id ?? "", upstreamModel: "" }],
              }))}>
              <PlusIcon />
            </Button>
          </Tooltip>
        </div>
      </div>
    </details>
  );
}

/** Structured catalog and mapping editor; no raw JSON anywhere. */
export function CodexModelSection({ editor, busy, userConfigModel, userConfigWarnings }: Props) {
  const { draft, setDraft, connection } = editor;
  const fetchIntoCatalog = async () => {
    const models = await connection.fetchModels();
    if (models) setDraft((current) => ({
      ...current,
      catalog: mergeFetchedCatalog(current.catalog, models, current.capabilities),
    }));
  };
  return (
    <section className="asb-provider-section" aria-label="模型">
      <h3 className="asb-section-title">模型</h3>
      <div className="asb-provider-section-fields">
        <div className="asb-provider-model-toolbar">
          <label className="asb-field">
            <span>默认模型</span>
            <Select ariaLabel="默认模型" value={draft.defaultModel || null} disabled={busy}
              placeholder={draft.catalog.length === 0 ? "请先添加模型" : "选择默认模型"}
              options={draft.catalog.map((entry) => ({ value: entry.id, label: entry.id }))}
              onChange={(value) => setDraft((current) => ({ ...current, defaultModel: value }))} />
          </label>
          <div className="asb-provider-model-actions" aria-label="模型目录操作">
            <Tooltip label={connection.modelsBusy ? "正在获取模型" : "获取模型"}>
              <Button variant="icon" aria-label={connection.modelsBusy ? "正在获取模型" : "获取模型"}
                aria-busy={connection.modelsBusy || undefined}
                disabled={busy || connection.modelsBusy || !connection.baseUrl}
                onClick={() => void fetchIntoCatalog()}>
                <UpdateIcon />
              </Button>
            </Tooltip>
            <Tooltip label="添加模型">
              <Button variant="icon" aria-label="添加模型" disabled={busy}
                onClick={() => setDraft((current) => ({
                  ...current,
                  catalog: [...current.catalog, emptyCatalogEntry(current.capabilities)],
                }))}>
                <PlusIcon />
              </Button>
            </Tooltip>
          </div>
        </div>
        <p className="asb-scope-note">客户端未指定模型时使用；请求映射与压缩能力都以目录为准。</p>
        {userConfigModel && <p className="asb-scope-note">当前用户级配置模型：{userConfigModel}</p>}
        {userConfigWarnings.map((warning) => (
          <p key={warning} className="asb-scope-note asb-warn-text">{warning}</p>
        ))}
        {connection.modelsError && <span className="asb-warn-text">{connection.modelsError}</span>}
        <p className="asb-scope-note">「获取模型」按已声明的供应商能力生成目录行；上下文窗口与输出上限留空即按官方参数（无则按通用默认值）保存，能力声明本身不会被推断。</p>
        <div className="asb-provider-catalog">
          {draft.catalog.map((entry, index) => (
            <CatalogRow key={index} editor={editor} busy={busy} entry={entry} index={index} />
          ))}
        </div>
        <ModelMapping editor={editor} busy={busy} />
      </div>
    </section>
  );
}
