import { useEffect, useState } from "react";
import type { CodexProviderRecord, CodexSubagentRoute, SettingSpec, SettingValue } from "../../api/client";
import { listCodexProfiles } from "../../api/providers";
import { Button } from "../Button";
import { Checkbox } from "../Checkbox";
import { ModelPicker } from "../ModelPicker";
import { RadioOption } from "../RadioOption";
import { SettingsFields, SettingsRow } from "../SettingsFields";
import { ParametersLoadStatus } from "../provider-editor/ProviderParametersPage";
import type { CodexEditorState } from "./useCodexProviderEditor";

interface Props {
  editor: CodexEditorState;
  busy: boolean;
  baselineValues?: Record<string, SettingValue>;
  /** The saved subagent route of the record being edited, if any. */
  baselineRoute?: CodexSubagentRoute | null;
  /** The record's profile id; absent while creating a new profile. */
  selfId?: string;
}

const automatic: SettingValue = { mode: "automatic" };

function sameRoute(left: CodexSubagentRoute | null | undefined, right: CodexSubagentRoute | null | undefined): boolean {
  if (!left || !right) return !left && !right;
  return left.profileId === right.profileId && left.model === right.model;
}

function routeLabel(route: CodexSubagentRoute | null | undefined, records: CodexProviderRecord[] | null): string {
  if (!route) return "自动";
  const owner = records?.find((record) => record.profile.id === route.profileId);
  return owner ? `${owner.profile.name} · ${route.model}` : `${route.profileId}/${route.model}`;
}

function SubagentModelRow({ baselineRoute, selfId, editor, busy }: {
  baselineRoute?: CodexSubagentRoute | null;
  selfId?: string;
  editor: CodexEditorState;
  busy: boolean;
}) {
  const { draft, setDraft } = editor;
  const route = draft.subagentRoute;
  const [records, setRecords] = useState<CodexProviderRecord[] | null>(null);
  const [recordsError, setRecordsError] = useState<string | null>(null);
  useEffect(() => {
    let cancelled = false;
    listCodexProfiles()
      .then((all) => {
        if (!cancelled) setRecords(all);
      })
      .catch(() => {
        if (!cancelled) setRecordsError("无法读取供应商档案列表，跨供应商选择暂不可用");
      });
    return () => {
      cancelled = true;
    };
  }, []);
  const setRoute = (next: CodexSubagentRoute | null) =>
    setDraft((current) => ({ ...current, subagentRoute: next }));
  const others = (records ?? []).filter((record) => record.profile.id !== selfId);
  const cross = route !== null && route.profileId !== selfId;
  const selfModels = draft.catalog
    .filter((entry) => entry.id.trim())
    .map((entry) => ({ id: entry.id, ownedBy: null, imageInput: entry.images ? true : null }));
  const otherModels = others.flatMap((record) =>
    record.profile.catalog.map((entry) => ({
      id: entry.id,
      ownedBy: record.profile.name,
      imageInput: entry.images ? true : null,
    })),
  );
  const models = cross ? otherModels : selfModels;
  const pick = (model: string) => {
    if (!cross) {
      if (selfId) setRoute({ profileId: selfId, model });
      return;
    }
    const owner = others.find((record) => record.profile.catalog.some((entry) => entry.id === model));
    if (owner) setRoute({ profileId: owner.profile.id, model });
  };
  const setCross = (enabled: boolean) => {
    if (!selfId) return;
    const keep = route?.model;
    if (enabled) {
      const owner = others.find((record) =>
        record.profile.catalog.some((entry) => entry.id === keep)) ?? others[0];
      const model = owner
        ? (owner.profile.catalog.find((entry) => entry.id === keep)?.id ?? owner.profile.catalog[0]?.id ?? "")
        : "";
      if (owner && model) setRoute({ profileId: owner.profile.id, model });
      return;
    }
    const model = draft.catalog.find((entry) => entry.id === keep)?.id ?? draft.defaultModel;
    setRoute({ profileId: selfId, model });
  };
  const target = route
    ? (route.profileId === selfId ? null : others.find((record) => record.profile.id === route.profileId))
    : null;
  const missingModel = target != null
    && route !== null
    && !target.profile.catalog.some((entry) => entry.id === route.model);
  const changed = !sameRoute(route, baselineRoute);
  return (
    <div className="asb-toggle-row asb-choice-row">
      <div className="asb-choice-head">
        <div className="asb-app-setting-copy">
          <span className="asb-checkbox-label">默认子 agent 模型</span>
          <span className="asb-app-setting-detail">以跨档案路由引用保存，经本机网关转发到目标档案；自动时使用供应商默认模型，任务或角色显式模型仍可覆盖。</span>
        </div>
        <span className="asb-setting-actual" aria-live="polite">当前设置：{routeLabel(baselineRoute, records)}</span>
      </div>
      <div className="asb-choice-controls">
        <div className="asb-segments" role="radiogroup" aria-label="默认子 agent 模型配置方式">
          <RadioOption name="subagent-route-mode" checked={route === null} disabled={busy} label="自动"
            onChange={() => setRoute(null)} />
          <RadioOption name="subagent-route-mode" checked={route !== null} disabled={busy} label="指定路由"
            onChange={() => {
              if (selfId) {
                setRoute({ profileId: selfId, model: draft.defaultModel });
              } else if (others.length > 0) {
                const model = others[0].profile.catalog[0]?.id ?? "";
                if (model) setRoute({ profileId: others[0].profile.id, model });
              }
            }} />
        </div>
        {route !== null && (
          <div className="asb-model-control asb-provider-subagent-model-control">
            <div className="asb-client-settings-reset-advanced">
              <Checkbox checked={cross} disabled={busy || !selfId} label="跨供应商选择（高级）"
                onChange={setCross} />
              <p className="asb-field-help">
                关闭时只列出本档案目录的模型；开启后从其他已保存档案的目录中选择。子代理请求由本机网关转发到目标档案，失败不会回退到主模型；配置路由后本档案将强制经网关路由。
              </p>
            </div>
            {models.length > 0 ? (
              <ModelPicker models={models} current={route.model || null}
                ariaLabel="选择子 agent 模型" disabled={busy}
                onSelect={pick} />
            ) : (
              <span className="asb-field-help">
                {cross ? "没有其他档案的模型目录可选" : "本档案目录为空；请先在「模型」分区添加模型"}
              </span>
            )}
            {recordsError && <span className="asb-warn-text">{recordsError}</span>}
            {target === null && route !== null && route.profileId !== selfId && (
              <span className="asb-warn-text">引用的供应商档案不存在；请重新选择</span>
            )}
            {missingModel && <span className="asb-warn-text">模型不在目标档案目录中；请重新选择</span>}
          </div>
        )}
      </div>
      {changed && (
        <p className="asb-setting-pending" role="status">
          {route === null ? "待保存：移除此项，恢复默认模型" : `待保存：写入「${routeLabel(route, records)}」`}
        </p>
      )}
    </div>
  );
}

function SubagentSettingsGroup({ specs, baselineValues, baselineRoute, selfId, editor, busy, onChange }: {
  specs: SettingSpec[];
  baselineValues: Record<string, SettingValue>;
  baselineRoute?: CodexSubagentRoute | null;
  selfId?: string;
  editor: CodexEditorState;
  busy: boolean;
  onChange: (key: string, value: SettingValue) => void;
}) {
  if (!editor.draft.parameters) return null;
  const settings = editor.draft.parameters.settings;
  return (
    <section className="asb-toggle-group">
      <div className="asb-toggle-group-head">
        <h3 className="asb-section-title">子 agent</h3>
      </div>
      <p className="asb-field-help">默认模型与推理强度随此供应商保存，并在切换到它时应用。</p>
      <SubagentModelRow baselineRoute={baselineRoute} selfId={selfId} editor={editor} busy={busy} />
      {specs.map((spec) => (
        <SettingsRow key={spec.key} spec={spec} value={settings[spec.key]}
          baselineValue={baselineValues[spec.key] ?? automatic} showActual={false} busy={busy}
          presentation="provider" onChange={(value) => onChange(spec.key, value)} />
      ))}
    </section>
  );
}

export function CodexParametersPage({ editor, busy, baselineValues, baselineRoute, selfId }: Props) {
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
        baselineRoute={baselineRoute} selfId={selfId}
        editor={editor} busy={busy} onChange={change} />
      <div className="asb-settings-actions">
        <Button variant="secondary" disabled={busy} onClick={resetAll}>恢复运行参数默认值</Button>
      </div>
    </div>
  );
}
