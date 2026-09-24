import { uiMessage } from "../../i18n/errors";
import { useMessageState } from "../../i18n/use-message-state";
import { useEffect, useState } from "react";
import type { CodexProviderRecord, CodexSubagentRoute } from "../../api/client";
import { listCodexProfiles } from "../../api/providers";
import type { TFunction } from "../../i18n";
import { useI18n } from "../../i18n";
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

function routeLabel(route: CodexSubagentRoute | null | undefined, records: CodexProviderRecord[], t: TFunction) {
  if (!route) return t("codex.subagent.auto");
  const owner = records.find(({ profile }) => profile.id === route.profileId);
  return owner ? t("codex.subagent.routeOwner", { name: owner.profile.name, model: route.model })
    : `${route.profileId}/${route.model}`;
}

function useSubagentModel({ baselineRoute, selfId, editor }: Props) {
  const { t } = useI18n();
  const { draft, setDraft } = editor;
  const route = draft.subagentRoute;
  const [records, setRecords] = useState<CodexProviderRecord[]>([]);
  const [loaded, setLoaded] = useState(false);
  const [error, setError] = useMessageState();
  useEffect(() => {
    let cancelled = false;
    listCodexProfiles().then((all) => {
      if (!cancelled) { setRecords(all); setLoaded(true); }
    }).catch(() => {
      if (!cancelled) setError(uiMessage("codex.subagent.listError"));
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
  const warning = error ?? (cross && loaded && !target ? t("codex.subagent.missingTarget")
    : target?.profile.connection?.authBinding ? t("codex.subagent.authBoundTarget")
    : target && !target.profile.catalog.some(({ id }) => id === route?.model)
      ? t("codex.subagent.modelNotInCatalog") : null);
  return { route, cross, models, pick, setCross, setRoute, specify, warning,
    current: route ? (cross ? subagentModelKey(route) : route.model) : null,
    changed: route?.profileId !== baselineRoute?.profileId || route?.model !== baselineRoute?.model,
    baselineLabel: routeLabel(baselineRoute, records, t), pendingLabel: routeLabel(route, records, t) };
}

function RouteSelection({ model, busy, selfId }: {
  model: ReturnType<typeof useSubagentModel>; busy: boolean; selfId?: string;
}) {
  const { t } = useI18n();
  return (
    <div className="asb-model-control asb-provider-subagent-model-control">
      <div className="asb-client-settings-reset-advanced">
        <Checkbox checked={model.cross} disabled={busy || !selfId} label={t("codex.subagent.crossProvider")}
          onChange={model.setCross} />
        <p className="asb-field-help">
          {t("codex.subagent.crossHelp")}
        </p>
      </div>
      {model.models.length > 0 ? (
        <ModelPicker models={model.models} current={model.current} ariaLabel={t("codex.subagent.pickerAria")}
          disabled={busy} onSelect={model.pick} />
      ) : <span className="asb-field-help">{model.cross ? t("codex.subagent.noOtherModels")
        : t("codex.subagent.emptyCatalog")}</span>}
      {model.warning && <span className="asb-warn-text">{model.warning}</span>}
    </div>
  );
}

export function SubagentModelRow(props: Props) {
  const { t } = useI18n();
  const model = useSubagentModel(props);
  return (
    <div className="asb-toggle-row asb-choice-row">
      <div className="asb-choice-head">
        <div className="asb-app-setting-copy">
          <span className="asb-checkbox-label">{t("codex.subagent.defaultModel")}</span>
          <span className="asb-app-setting-detail">{t("codex.subagent.defaultDetail")}</span>
        </div>
        <span className="asb-setting-actual" aria-live="polite">{t("codex.subagent.currentSetting", { value: model.baselineLabel })}</span>
      </div>
      <div className="asb-choice-controls">
        <div className="asb-segments" role="radiogroup" aria-label={t("codex.subagent.modeAria")}>
          <RadioOption name="subagent-route-mode" checked={model.route === null} disabled={props.busy}
            label={t("codex.subagent.auto")} onChange={() => model.setRoute(null)} />
          <RadioOption name="subagent-route-mode" checked={model.route !== null} disabled={props.busy}
            label={t("codex.subagent.specify")} onChange={model.specify} />
        </div>
        {model.route !== null && <RouteSelection model={model} busy={props.busy} selfId={props.selfId} />}
      </div>
      {model.changed && <p className="asb-setting-pending" role="status">
        {model.route === null ? t("codex.subagent.pendingRemove") : t("codex.subagent.pendingWrite", { value: model.pendingLabel })}
      </p>}
    </div>
  );
}
