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
  const [draft, setDraft] = useState<UsageQuery | null>(() => value);
  const [querying, setQuerying] = useState(false);
  const [saving, setSaving] = useState(false);
  const [summary, setSummary] = useState<UsageSummary | null>(null);
  const [error, setError] = useState<string | null>(null);
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
      setError("供应商缺少 API 格式，无法查询用量");
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
        setError((caught as { message?: string }).message ?? "查询失败");
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
    <EditorFrame className="asb-usage-editor" title="用量查询" backLabel="返回供应商" busy={controlsDisabled} onBack={onClose}
      primary={
        <>
          <p className="asb-usage-provider">{providerName.trim() || "未命名供应商"}</p>
          <Tabs value={kind} onChange={selectKind} scope={modeScope} label="查询方式"
            tabs={[
              { value: "declarative" as const, label: "字段提取", disabled: controlsDisabled,
                controls: `${modeScope}-declarative-panel` },
              { value: "script" as const, label: "自编脚本", disabled: controlsDisabled,
                controls: `${modeScope}-script-panel` },
            ]} />
        </>
      }
      footer={
        <Button variant="primary" className="asb-editor-submit" disabled={controlsDisabled} onClick={() => void save()}>
          {saving ? "保存中…" : "保存查询"}
        </Button>
      }>
      <section className="asb-usage-workspace" aria-label="用量查询"
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
              <h3 className="asb-section-title">查询配置</h3>
              <div className="asb-editor-section-fields">
                <label className="asb-field">
                  <span>查询地址</span>
                  <Input
                    aria-label="用量查询地址"
                    value={declarative.url}
                    disabled={controlsDisabled}
                    placeholder="{{baseUrl}}/user/balance"
                    onChange={(event) => patchDeclarative({ url: event.target.value })}
                  />
                </label>
                <div className="asb-usage-paths">
                  <label className="asb-field">
                    <span>余额路径</span>
                    <Input
                      code
                      aria-label="余额提取路径"
                      value={declarative.remainingPath ?? ""}
                      disabled={controlsDisabled}
                      placeholder="data/balance"
                      onChange={(event) => patchDeclarative({ remainingPath: optional(event.target.value) })}
                    />
                  </label>
                  <label className="asb-field">
                    <span>已用路径</span>
                    <Input
                      code
                      aria-label="已用提取路径"
                      value={declarative.usedPath ?? ""}
                      disabled={controlsDisabled}
                      placeholder="data/used"
                      onChange={(event) => patchDeclarative({ usedPath: optional(event.target.value) })}
                    />
                  </label>
                  <label className="asb-field">
                    <span>总量路径</span>
                    <Input
                      code
                      aria-label="总量提取路径"
                      value={declarative.totalPath ?? ""}
                      disabled={controlsDisabled}
                      placeholder="data/total"
                      onChange={(event) => patchDeclarative({ totalPath: optional(event.target.value) })}
                    />
                  </label>
                  <label className="asb-field">
                    <span>单位</span>
                    <Input
                      aria-label="用量单位"
                      value={declarative.unit ?? ""}
                      disabled={controlsDisabled}
                      placeholder="USD"
                      onChange={(event) => patchDeclarative({ unit: optional(event.target.value) })}
                    />
                  </label>
                </div>
                <p className="asb-scope-note">
                  以一次 GET 请求读取 JSON；地址可使用 {"{{baseUrl}}"} 与 {"{{apiKey}}"}。
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
              <h3 className="asb-section-title">查询配置</h3>
              <div className="asb-editor-section-fields">
                <label className="asb-field">
                  <span>用量查询脚本</span>
                  <Textarea
                    code
                    aria-label="用量查询脚本"
                    rows={16}
                    value={draft?.kind === "script" ? draft.source : ""}
                    disabled={controlsDisabled}
                    placeholder={SCRIPT_TEMPLATE}
                    spellCheck={false}
                    onChange={(event) => patchScript(event.target.value)}
                  />
                </label>
                <div className="asb-usage-script-contract">
                  <span>输入</span>
                  <code>{"request({ baseUrl, apiKey })"}</code>
                  <span>输出</span>
                  <code>{"extract({ body, status })"}</code>
                </div>
                <p className="asb-scope-note">
                  脚本只能生成一次 GET / POST 请求并提取 JSON 数值；网络请求由应用执行。
                </p>
              </div>
            </>
          )}
        </div>

        <section className="asb-editor-section" aria-label="自动刷新">
          <h3 className="asb-section-title">自动刷新</h3>
          <div className="asb-editor-section-fields">
            <label className="asb-field is-narrow">
              <span>间隔（分钟，0 为关闭）</span>
              <Input
                type="number"
                min={0}
                max={1440}
                step={1}
                aria-label="自动刷新间隔（分钟，0 为关闭）"
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
        <section className="asb-editor-section" aria-label="查询测试">
          <h3 className="asb-section-title">查询测试</h3>
          <div className="asb-editor-section-fields">
            <div className="asb-editor-action-row">
              <Button
                variant="secondary"
                className="asb-usage-run"
                disabled={controlsDisabled || !canRun(draft)}
                onClick={() => void run()}
              >
                <UsageIcon />
                {querying ? "查询中…" : "查询用量"}
              </Button>
              <span className="asb-field-help">
                {querying ? "正在查询" : canRun(draft) ? "准备就绪" : "请先完成查询配置"}
              </span>
            </div>
            {error && <p className="asb-warn-text" role="alert">{error}</p>}
          </div>
        </section>

        {summary && (
          <section className="asb-editor-section" aria-label="本次用量结果">
            <h3 className="asb-section-title">本次结果</h3>
            <div className="asb-editor-section-fields">
              <div className="asb-usage-readout-head">
                <Time iso={summary.at} />
              </div>
              <UsageReadingsTable readings={summary.readings} ariaLabel="本次用量读数" />
            </div>
          </section>
        )}
      </section>
    </EditorFrame>
  );
}
