import { useEffect, useState } from "react";
import type { CodexProviderRecord, CodexSubagentRoute } from "../../api/client";
import { listCodexProfiles } from "../../api/providers";
import { Checkbox } from "../Checkbox";
import { ModelPicker } from "../ModelPicker";
import { RadioOption } from "../RadioOption";
import type { CodexEditorState } from "./useCodexProviderEditor";
import { subagentModelKey, subagentModelOptions } from "./subagent-model-options";

interface Props {
  baselineRoute?: CodexSubagentRoute | null;
  selfId?: string;
  editor: CodexEditorState;
  busy: boolean;
}

function routeLabel(route: CodexSubagentRoute | null | undefined, records: CodexProviderRecord[]) {
  if (!route) return "自动";
  const owner = records.find(({ profile }) => profile.id === route.profileId);
  return owner ? `${owner.profile.name} · ${route.model}` : `${route.profileId}/${route.model}`;
}

function useSubagentModel({ baselineRoute, selfId, editor }: Props) {
  const { draft, setDraft } = editor;
  const route = draft.subagentRoute;
  const [records, setRecords] = useState<CodexProviderRecord[]>([]);
  const [loaded, setLoaded] = useState(false);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    let cancelled = false;
    listCodexProfiles().then((all) => {
      if (!cancelled) { setRecords(all); setLoaded(true); }
    }).catch(() => {
      if (!cancelled) setError("无法读取供应商档案列表，跨供应商选择暂不可用");
    });
    return () => { cancelled = true; };
  }, []);
  const setRoute = (next: CodexSubagentRoute | null) =>
    setDraft((current) => ({ ...current, subagentRoute: next }));
  const cross = route !== null && route.profileId !== selfId;
  const others = subagentModelOptions(records, selfId);
  const models = cross ? others : draft.catalog.filter(({ id }) => id.trim())
    .map(({ id }) => ({ value: id, label: id, group: null }));
  const pick = (value: string) => {
    if (!cross && selfId) setRoute({ profileId: selfId, model: value });
    if (cross) {
      const selected = others.find((option) => option.value === value);
      if (selected) setRoute(selected.route);
    }
  };
  const setCross = (enabled: boolean) => {
    if (!selfId) return;
    if (enabled) {
      const selected = others.find((option) => option.route.model === route?.model) ?? others[0];
      if (selected) setRoute(selected.route);
    } else {
      const model = draft.catalog.find(({ id }) => id === route?.model)?.id ?? draft.defaultModel;
      setRoute({ profileId: selfId, model });
    }
  };
  const specify = () => {
    if (selfId) setRoute({ profileId: selfId, model: draft.defaultModel });
    else if (others[0]) setRoute(others[0].route);
  };
  const target = cross ? records.find(({ profile }) => profile.id === route?.profileId) : null;
  const warning = error ?? (cross && loaded && !target ? "引用的供应商档案不存在；请重新选择"
    : target?.profile.connection?.authBinding ? "目标档案绑定了账号凭据；请重新选择"
    : target && !target.profile.catalog.some(({ id }) => id === route?.model)
      ? "模型不在目标档案目录中；请重新选择" : null);
  return { route, cross, models, pick, setCross, setRoute, specify, warning,
    current: route ? (cross ? subagentModelKey(route) : route.model) : null,
    changed: route?.profileId !== baselineRoute?.profileId || route?.model !== baselineRoute?.model,
    baselineLabel: routeLabel(baselineRoute, records), pendingLabel: routeLabel(route, records) };
}

function RouteSelection({ model, busy, selfId }: {
  model: ReturnType<typeof useSubagentModel>; busy: boolean; selfId?: string;
}) {
  return (
    <div className="asb-model-control asb-provider-subagent-model-control">
      <div className="asb-client-settings-reset-advanced">
        <Checkbox checked={model.cross} disabled={busy || !selfId} label="跨供应商选择（高级）"
          onChange={model.setCross} />
        <p className="asb-field-help">
          关闭时只列出本档案目录的模型；开启后从其他已保存且未绑定账号的档案中选择。子代理请求由本机网关转发到目标档案，失败不会回退到主模型；配置路由后本档案将强制经网关路由。
        </p>
      </div>
      {model.models.length > 0 ? (
        <ModelPicker models={model.models} current={model.current} ariaLabel="选择子 agent 模型"
          disabled={busy} onSelect={model.pick} />
      ) : <span className="asb-field-help">{model.cross ? "没有其他档案的模型目录可选"
        : "本档案目录为空；请先在「模型」分区添加模型"}</span>}
      {model.warning && <span className="asb-warn-text">{model.warning}</span>}
    </div>
  );
}

export function SubagentModelRow(props: Props) {
  const model = useSubagentModel(props);
  return (
    <div className="asb-toggle-row asb-choice-row">
      <div className="asb-choice-head">
        <div className="asb-app-setting-copy">
          <span className="asb-checkbox-label">默认子 agent 模型</span>
          <span className="asb-app-setting-detail">以跨档案路由引用保存，经本机网关转发到目标档案；自动时使用供应商默认模型，任务或角色显式模型仍可覆盖。</span>
        </div>
        <span className="asb-setting-actual" aria-live="polite">当前设置：{model.baselineLabel}</span>
      </div>
      <div className="asb-choice-controls">
        <div className="asb-segments" role="radiogroup" aria-label="默认子 agent 模型配置方式">
          <RadioOption name="subagent-route-mode" checked={model.route === null} disabled={props.busy}
            label="自动" onChange={() => model.setRoute(null)} />
          <RadioOption name="subagent-route-mode" checked={model.route !== null} disabled={props.busy}
            label="指定路由" onChange={model.specify} />
        </div>
        {model.route !== null && <RouteSelection model={model} busy={props.busy} selfId={props.selfId} />}
      </div>
      {model.changed && <p className="asb-setting-pending" role="status">
        {model.route === null ? "待保存：移除此项，恢复默认模型" : `待保存：写入「${model.pendingLabel}」`}
      </p>}
    </div>
  );
}
