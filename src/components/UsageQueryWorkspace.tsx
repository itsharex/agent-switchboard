import { uiMessage } from "../i18n/errors";
import { useMessageState } from "../i18n/use-message-state";
import { useEffect, useId, useRef, useState } from "react";
import {
  testUsageQuery,
  type DeclarativeUsageQuery,
  type ProviderConnectionOptions,
  type UpstreamProtocol,
  type UsageQuery,
  type UsageSummary,
} from "../api/client";
import { Input } from "./Input";
import { Tabs } from "./Tabs";
import { Time } from "./Time";
import { Textarea } from "./Textarea";
import { Button } from "./Button";
import { EditorFrame } from "./EditorFrame";
import { UsageIcon } from "./icons";
import { useI18n } from "../i18n";
import { UsageReadingsTable } from "./UsageReadingsTable";
import { normalizeUsageQuery } from "../lib/usage-query";

interface Props {
  providerName: string;
  value: UsageQuery | null;
  apiKey: string;
  authentication?: import("../api/shared").AuthenticationScheme | null;
  connection?: ProviderConnectionOptions | null;
  baseUrl: string | null;
  upstreamProtocol: UpstreamProtocol | null;
  busy: boolean;
  onSave: (next: UsageQuery | null) => Promise<boolean> | boolean;
  onClose: () => void;
}

const SCRIPT_TEMPLATE = `({
  request: ({ baseUrl, apiKey }) => ({
    url: \`${"${baseUrl}"}/user/balance\`,
    method: "GET",
    headers: { Authorization: \`Bearer ${"${apiKey}"}\` },
  }),
  extract: ({ body }) => ({
    remaining: body.balance,
    used: null,
    total: null,
    unit: "USD",
  }),
})`;

function emptyDeclarative(interval: number): DeclarativeUsageQuery {
  return {
    kind: "declarative",
    url: "",
    remainingPath: null,
    usedPath: null,
    totalPath: null,
    unit: null,
    refreshIntervalMinutes: interval,
  };
}

function canRun(query: UsageQuery | null): boolean {
  if (!query) return false;
  return query.kind === "declarative" ? Boolean(query.url.trim()) : Boolean(query.source.trim());
}

function optional(raw: string): string | null {
  const value = raw.trim();
  return value || null;
}

/**
 * Dedicated settings workspace for one provider's optional usage query. It
 * owns the draft and transient result; its caller owns the persisted profile.
 */
export function UsageQueryWorkspace({
  providerName,
  value,
  apiKey,
  authentication,
  connection,
  baseUrl,
  upstreamProtocol,
  busy,
  onSave,
  onClose,
}: Props) {
  const { t } = useI18n();
  const [draft, setDraft] = useState<UsageQuery | null>(() => value);
  const [querying, setQuerying] = useState(false);
  const [saving, setSaving] = useState(false);
  const [summary, setSummary] = useState<UsageSummary | null>(null);
  const [error, setError] = useMessageState();
  const queryVersion = useRef(0);
  const firstRun = useRef(true);

  const clearResult = () => {
    queryVersion.current += 1;
    setSummary(null);
    setError(null);
  };

  const run = async () => {
    if (!draft || querying || saving) return;
    if (!upstreamProtocol) {
      setError(uiMessage("usage.query.missingProtocol"));
      return;
    }
    const version = ++queryVersion.current;
    setQuerying(true);
    setError(null);
    try {
      const next = await testUsageQuery(
        draft,
        apiKey,
        baseUrl,
        upstreamProtocol,
        authentication,
        connection,
      );
      if (queryVersion.current === version) setSummary(next);
    } catch (caught) {
      if (queryVersion.current === version) {
        setSummary(null);
        setError(caught);
      }
    } finally {
      if (queryVersion.current === version) setQuerying(false);
    }
  };

  // Entering a configured workspace reads once. Empty optional
  // configurations only open the editor.
  useEffect(() => {
    if (!firstRun.current) return;
    firstRun.current = false;
    if (canRun(draft)) void run();
    // This subview mounts for each explicit open. Subsequent edits require an
    // explicit query, rather than issuing network requests for every keystroke.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const kind = draft?.kind ?? "declarative";
  const declarative = draft?.kind === "declarative" ? draft : emptyDeclarative(0);
  const intervalMinutes = draft ? draft.refreshIntervalMinutes : 0;
  const [intervalText, setIntervalText] = useState(() => String(intervalMinutes));

  const selectKind = (next: "declarative" | "script") => {
    if (next === kind) return;
    clearResult();
    // The refresh interval is mode-independent and survives a mode switch.
    setDraft(
      next === "declarative"
        ? emptyDeclarative(intervalMinutes)
        : { kind: "script", source: "", refreshIntervalMinutes: intervalMinutes },
    );
  };

  const patchDeclarative = (fields: Partial<DeclarativeUsageQuery>) => {
    clearResult();
    setDraft({ ...declarative, ...fields, kind: "declarative" });
  };

  const patchScript = (source: string) => {
    clearResult();
    setDraft({ kind: "script", source, refreshIntervalMinutes: intervalMinutes });
  };

  /** Commits the free-form interval into the draft; anything outside whole
   * minutes within 0–1440 reverts to the draft's persisted value. */
  const commitInterval = () => {
    const text = intervalText.trim();
    if (/^\d+$/.test(text) && Number(text) <= 1440) {
      const minutes = Number(text);
      setIntervalText(String(minutes));
      if (minutes !== intervalMinutes) {
        setDraft((current) =>
          current && current.kind === "script"
            ? { ...current, refreshIntervalMinutes: minutes }
            : { ...declarative, kind: "declarative", refreshIntervalMinutes: minutes },
        );
      }
    } else {
      setIntervalText(String(intervalMinutes));
    }
  };

  const save = async () => {
    if (busy || querying || saving) return;
    setSaving(true);
    try {
      await onSave(normalizeUsageQuery(draft));
    } finally {
      setSaving(false);
    }
  };

  const controlsDisabled = busy || querying || saving;
  const modeScope = useId();

  return (
    <EditorFrame className="asb-usage-editor" title={t("usage.query.title")} backLabel={t("usage.query.back")} busy={controlsDisabled} onBack={onClose}
      primary={
        <>
          <p className="asb-usage-provider">{providerName.trim() || t("usage.query.unnamedProvider")}</p>
          <Tabs value={kind} onChange={selectKind} scope={modeScope} label={t("usage.query.modeAria")}
            tabs={[
              { value: "declarative" as const, label: t("usage.query.modeDeclarative"), disabled: controlsDisabled,
                controls: `${modeScope}-declarative-panel` },
              { value: "script" as const, label: t("usage.query.modeScript"), disabled: controlsDisabled,
                controls: `${modeScope}-script-panel` },
            ]} />
        </>
      }
      footer={
        <Button variant="primary" className="asb-editor-submit" disabled={controlsDisabled} onClick={() => void save()}>
          {saving ? t("usage.query.saving") : t("usage.query.save")}
        </Button>
      }>
      <section className="asb-usage-workspace" aria-label={t("usage.query.title")}
        onKeyDown={(event) => {
          if (event.key === "Escape" && !querying && !saving && !busy) onClose();
        }}>
        {/* Both tabpanels stay mounted so each tab's aria-controls always
            resolves; the inactive editor unmounts inside its hidden card. */}
        <div
          className="asb-editor-section"
          role="tabpanel"
          id={`${modeScope}-declarative-panel`}
          aria-labelledby={`${modeScope}-declarative-tab`}
          hidden={kind !== "declarative"}
        >
          {kind === "declarative" && (
            <>
              <h3 className="asb-section-title">{t("usage.query.configTitle")}</h3>
              <div className="asb-editor-section-fields">
                <label className="asb-field">
                  <span>{t("usage.query.urlLabel")}</span>
                  <Input
                    aria-label={t("usage.query.urlAria")}
                    value={declarative.url}
                    disabled={controlsDisabled}
                    placeholder="{{baseUrl}}/user/balance"
                    onChange={(event) => patchDeclarative({ url: event.target.value })}
                  />
                </label>
                <div className="asb-usage-paths">
                  <label className="asb-field">
                    <span>{t("usage.query.remainingPath")}</span>
                    <Input
                      code
                      aria-label={t("usage.query.remainingPathAria")}
                      value={declarative.remainingPath ?? ""}
                      disabled={controlsDisabled}
                      placeholder="data/balance"
                      onChange={(event) => patchDeclarative({ remainingPath: optional(event.target.value) })}
                    />
                  </label>
                  <label className="asb-field">
                    <span>{t("usage.query.usedPath")}</span>
                    <Input
                      code
                      aria-label={t("usage.query.usedPathAria")}
                      value={declarative.usedPath ?? ""}
                      disabled={controlsDisabled}
                      placeholder="data/used"
                      onChange={(event) => patchDeclarative({ usedPath: optional(event.target.value) })}
                    />
                  </label>
                  <label className="asb-field">
                    <span>{t("usage.query.totalPath")}</span>
                    <Input
                      code
                      aria-label={t("usage.query.totalPathAria")}
                      value={declarative.totalPath ?? ""}
                      disabled={controlsDisabled}
                      placeholder="data/total"
                      onChange={(event) => patchDeclarative({ totalPath: optional(event.target.value) })}
                    />
                  </label>
                  <label className="asb-field">
                    <span>{t("usage.query.unit")}</span>
                    <Input
                      aria-label={t("usage.query.unitAria")}
                      value={declarative.unit ?? ""}
                      disabled={controlsDisabled}
                      placeholder="USD"
                      onChange={(event) => patchDeclarative({ unit: optional(event.target.value) })}
                    />
                  </label>
                </div>
                <p className="asb-scope-note">
                  {t("usage.query.declarativeNote", { baseUrl: "{{baseUrl}}", apiKey: "{{apiKey}}" })}
                </p>
                <p className="asb-scope-note">
                  {t("usage.query.fieldsNote")}
                </p>
              </div>
            </>
          )}
        </div>
        <div
          className="asb-editor-section"
          role="tabpanel"
          id={`${modeScope}-script-panel`}
          aria-labelledby={`${modeScope}-script-tab`}
          hidden={kind !== "script"}
        >
          {kind === "script" && (
            <>
              <h3 className="asb-section-title">{t("usage.query.configTitle")}</h3>
              <div className="asb-editor-section-fields">
                <label className="asb-field">
                  <span>{t("usage.query.scriptLabel")}</span>
                  <Textarea
                    code
                    aria-label={t("usage.query.scriptLabel")}
                    rows={16}
                    value={draft?.kind === "script" ? draft.source : ""}
                    disabled={controlsDisabled}
                    placeholder={SCRIPT_TEMPLATE}
                    spellCheck={false}
                    onChange={(event) => patchScript(event.target.value)}
                  />
                </label>
                <div className="asb-usage-script-contract">
                  <span>{t("usage.query.input")}</span>
                  <code>{"request({ baseUrl, apiKey })"}</code>
                  <span>{t("usage.query.output")}</span>
                  <code>{"extract({ body, status })"}</code>
                </div>
                <p className="asb-scope-note">
                  {t("usage.query.scriptNote")}
                </p>
              </div>
            </>
          )}
        </div>

        <section className="asb-editor-section" aria-label={t("usage.query.autoRefresh")}>
          <h3 className="asb-section-title">{t("usage.query.autoRefresh")}</h3>
          <div className="asb-editor-section-fields">
            <label className="asb-field is-narrow">
              <span>{t("usage.query.intervalLabel")}</span>
              <Input
                type="number"
                min={0}
                max={1440}
                step={1}
                aria-label={t("usage.query.intervalAria")}
                value={intervalText}
                disabled={controlsDisabled}
                onChange={(event) => setIntervalText(event.target.value)}
                onBlur={commitInterval}
                onKeyDown={(event) => {
                  if (event.key === "Enter") commitInterval();
                }}
              />
            </label>
          </div>
        </section>

        {/* The run action stays in the module area (DESIGN.md: the commit
            action alone lives in the frame's fixed bar); its error sits in the
            same card, directly under the trigger it explains. */}
        <section className="asb-editor-section" aria-label={t("usage.query.testTitle")}>
          <h3 className="asb-section-title">{t("usage.query.testTitle")}</h3>
          <div className="asb-editor-section-fields">
            <div className="asb-editor-action-row">
              <Button
                variant="secondary"
                className="asb-usage-run"
                disabled={controlsDisabled || !canRun(draft)}
                onClick={() => void run()}
              >
                <UsageIcon />
                {querying ? t("usage.query.running") : t("usage.query.run")}
              </Button>
              <span className="asb-field-help">
                {querying ? t("usage.query.querying") : canRun(draft) ? t("usage.query.ready") : t("usage.query.needConfig")}
              </span>
            </div>
            {error && <p className="asb-warn-text" role="alert">{error}</p>}
          </div>
        </section>

        {summary && (
          <section className="asb-editor-section" aria-label={t("usage.query.resultAria")}>
            <h3 className="asb-section-title">{t("usage.query.resultTitle")}</h3>
            <div className="asb-editor-section-fields">
              <div className="asb-usage-readout-head">
                <Time iso={summary.at} />
              </div>
              <UsageReadingsTable readings={summary.readings} ariaLabel={t("usage.query.readingsAria")} />
            </div>
          </section>
        )}
      </section>
    </EditorFrame>
  );
}
