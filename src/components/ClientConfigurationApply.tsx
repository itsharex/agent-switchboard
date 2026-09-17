import { useState } from "react";

import {
  commitClientConfigurationApply,
  commitClientConfigurationReset,
  previewClientConfigurationApply,
  previewClientConfigurationReset,
  type AppKind,
  type ClientConfigurationApplyPreview,
  type ClientConfigurationResetKind,
  type CodexSubagentSettings,
  type ConfigFileStatus,
  type SettingsValues,
} from "../api/client";
import { clientSettingsPayload } from "../app/claude-common-settings";
import type { ClientSettingsEditorState } from "../app/useClientSettings";
import { Button } from "./Button";
import { ConfirmSheet } from "./ConfirmSheet";
import { DiffView } from "./DiffView";
import { PreviewInspector } from "./PreviewInspector";

const CLAUDE_EXTRA_MANIFEST_KEYS = new Set([
  "/env/ASB_CLAUDE_COMMON_KEYS",
  "env.ASB_CLAUDE_COMMON_KEYS",
]);

function visibleResetChanges(
  resetKind: ClientConfigurationResetKind,
  preview: ClientConfigurationApplyPreview,
) {
  const changes = preview.file.preview.changes;
  return resetKind === "clearExtraConfiguration"
    ? changes.filter((change) => !CLAUDE_EXTRA_MANIFEST_KEYS.has(change.key))
    : changes;
}

function NativeResetScope({ app }: { app: AppKind }) {
  return (
    <div className="asb-client-settings-reset-scope">
      <span className="asb-client-settings-reset-scope-label">将恢复</span>
      <ul>
        <li>ASB 管理的标准客户端通用设置</li>
        {app === "codex" && <li>Codex 的 3 项子 agent 全局运行设置</li>}
      </ul>
      <p className="asb-field-help">
        不会影响供应商参数、全局指令、登录与凭据、扩展配置、未管理字段或额外通用配置。
      </p>
    </div>
  );
}

function ExtraResetScope({ extraConfigurationCount }: { extraConfigurationCount: number }) {
  return (
    <div className="asb-client-settings-reset-scope">
      <span className="asb-client-settings-reset-scope-label">将清空</span>
      <p>通过配置草稿由 ASB 管理的 {extraConfigurationCount} 项额外配置。</p>
      <p className="asb-field-help">
        不会影响标准通用设置、供应商参数、全局指令、登录与凭据、扩展配置或未管理字段。
      </p>
    </div>
  );
}

export function ClientConfigurationResetPreview({
  preview,
  app,
  resetKind,
  extraConfigurationCount,
  busy,
  onConfirm,
  onBack,
}: {
  preview: ClientConfigurationApplyPreview;
  app: AppKind;
  resetKind: ClientConfigurationResetKind;
  extraConfigurationCount: number;
  busy: boolean;
  onConfirm: () => void;
  onBack?: () => void;
}) {
  const nativeDefaults = resetKind === "nativeDefaults";
  const changes = visibleResetChanges(resetKind, preview);
  const title = nativeDefaults ? "恢复为客户端原生默认值" : "清空 ASB 管理的额外通用配置";
  const changeLabel = nativeDefaults ? "将移除的标准设置" : "将移除的额外配置";
  const confirmLabel = nativeDefaults
    ? "确认恢复原生默认值"
    : `确认清空 ${extraConfigurationCount} 项额外配置`;
  return (
    <section className="asb-client-settings-reset-preview" aria-label={`${title}预览`}>
      <h3 className="asb-section-title">{title}</h3>
      <p className="asb-client-settings-reset-lead">
        {nativeDefaults
          ? "清除 ASB 管理的标准通用配置覆盖，让客户端按其原生默认行为运行。"
          : "移除通过配置草稿由 ASB 明确管理的额外字段。"}
      </p>
      {nativeDefaults
        ? <NativeResetScope app={app} />
        : <ExtraResetScope extraConfigurationCount={extraConfigurationCount} />}
      <p className="asb-client-settings-reset-target">
        <span>目标文件</span>
        <code>{preview.file.preview.target}</code>
      </p>
      {changes.length > 0 ? (
        <div className="asb-client-settings-reset-changes">
          <DiffView changes={changes} label={changeLabel} />
        </div>
      ) : (
        <p className="asb-field-help">
          真实客户端文件当前没有需要修改的受管字段；确认后仍会更新 ASB 已保存的配置，避免下次应用重新写入覆盖。
        </p>
      )}
      {preview.file.preview.warnings.map((warning) => (
        <p key={warning} className="asb-field-error" role="alert">{warning}</p>
      ))}
      <p className="asb-field-help">
        真实文件需要变更时会先创建备份，并在同一可恢复事务中更新已保存的配置。
      </p>
      <div className="asb-client-settings-reset-actions">
        {onBack && (
          <Button variant="secondary" className="asb-client-settings-reset-back" disabled={busy} onClick={onBack}>
            返回恢复设置
          </Button>
        )}
        <Button variant={nativeDefaults ? "primary" : "danger"} disabled={busy} onClick={onConfirm}>
          {confirmLabel}
        </Button>
      </div>
    </section>
  );
}

export interface PendingClientConfiguration {
  preview: ClientConfigurationApplyPreview;
  settings: SettingsValues;
  subagentSettings?: CodexSubagentSettings;
}

export interface PendingClientConfigurationReset {
  preview: ClientConfigurationApplyPreview;
  resetKind: ClientConfigurationResetKind;
}

interface UseClientConfigurationApplyProps {
  app: AppKind;
  busy: boolean;
  editorState: ClientSettingsEditorState;
  subagentDraft?: CodexSubagentSettings;
  configStatus?: ConfigFileStatus;
  hasUnsavedConfigurationDraft: boolean;
  onApplied: () => void;
}

interface ConfigurationStatus {
  message: string;
  error: boolean;
}

function extraConfigurationCount(extra: Record<string, unknown> | undefined): number {
  if (!extra) return 0;
  const count = (value: unknown): number => {
    if (!value || typeof value !== "object" || Array.isArray(value)) return 1;
    const entries = Object.values(value);
    return entries.length === 0 ? 0 : entries.reduce<number>((total, child) => total + count(child), 0);
  };
  return Object.values(extra).reduce<number>((total, value) => total + count(value), 0);
}

type ResetState<T> = Record<ClientConfigurationResetKind, T>;

function emptyResetState<T>(value: T): ResetState<T> {
  return { nativeDefaults: value, clearExtraConfiguration: value };
}

interface UseClientConfigurationResetsProps {
  app: AppKind;
  busy: boolean;
  canReset: boolean;
  extraConfigurationCount: number;
  onStart: () => void;
  onFinish: () => void;
  onSuccess: (message: string) => void;
}

function useClientConfigurationResets({
  app,
  busy,
  canReset,
  extraConfigurationCount,
  onStart,
  onFinish,
  onSuccess,
}: UseClientConfigurationResetsProps) {
  const [pending, setPending] = useState<ResetState<PendingClientConfigurationReset | null>>(
    () => emptyResetState(null),
  );
  const [errors, setErrors] = useState<ResetState<string | null>>(() => emptyResetState(null));
  const [resetView, setResetView] = useState<ClientConfigurationResetKind>("nativeDefaults");
  const setPendingFor = (kind: ClientConfigurationResetKind, value: PendingClientConfigurationReset | null) => {
    setPending((current) => ({ ...current, [kind]: value }));
  };
  const setErrorFor = (kind: ClientConfigurationResetKind, value: string | null) => {
    setErrors((current) => ({ ...current, [kind]: value }));
  };
  const prepare = (resetKind: ClientConfigurationResetKind) => {
    if (!canReset || busy) return;
    setResetView(resetKind); setPendingFor(resetKind, null); setErrorFor(resetKind, null); onStart();
    void previewClientConfigurationReset(app, resetKind).then((preview) => {
      setPendingFor(resetKind, { preview, resetKind });
    }).catch((error: { message?: string }) => {
      setErrorFor(resetKind, error.message ?? "无法生成配置预览");
    }).finally(onFinish);
  };
  const commit = (resetKind: ClientConfigurationResetKind) => {
    const current = pending[resetKind];
    if (!current || busy) return;
    setErrorFor(resetKind, null); onStart();
    void commitClientConfigurationReset(app, resetKind, current.preview).then(() => {
      setPending(emptyResetState(null));
      onSuccess(resetKind === "nativeDefaults"
        ? "已恢复为客户端原生默认值。"
        : `已清空 ${extraConfigurationCount} 项 ASB 管理的额外通用配置。`);
    }).catch((error: { message?: string }) => {
      setErrorFor(resetKind, error.message ?? "应用客户端配置操作失败");
    }).finally(onFinish);
  };
  return {
    resetView,
    pendingNativeReset: pending.nativeDefaults,
    pendingExtraClear: pending.clearExtraConfiguration,
    nativeResetError: errors.nativeDefaults,
    extraClearError: errors.clearExtraConfiguration,
    prepareNativeReset: () => prepare("nativeDefaults"),
    prepareExtraClear: () => prepare("clearExtraConfiguration"),
    commitNativeReset: () => commit("nativeDefaults"),
    commitExtraClear: () => commit("clearExtraConfiguration"),
    showNativeReset: () => setResetView("nativeDefaults"),
  };
}

export function useClientConfigurationApply(
  props: UseClientConfigurationApplyProps,
  onCommitted: () => void,
) {
  const { editorState: state, app, busy } = props;
  const [pendingApply, setPendingApply] = useState<PendingClientConfiguration | null>(null);
  const [applyError, setApplyError] = useState<string | null>(null);
  const [status, setStatus] = useState<ConfigurationStatus | null>(null);
  const [applying, setApplying] = useState(false);
  const settings: SettingsValues | null = state.editor && state.draft
    ? clientSettingsPayload(app, state.draft, state.claudeExtra) : null;
  const subagentSettings = app === "codex" ? props.subagentDraft : undefined;
  const fileIsWritable = !props.configStatus?.exists || props.configStatus.syntaxOk;
  const recoveryBlocker = state.phase === "dirty" || props.hasUnsavedConfigurationDraft
    ? "存在未保存的配置草稿。请先保存，或重新加载以放弃草稿后，再执行恢复或清空操作。"
    : null;
  const recoveryReady = fileIsWritable && state.phase === "clean" && !recoveryBlocker;
  const extraCount = app === "claude" ? extraConfigurationCount(state.editor?.settings.claudeExtra) : 0;
  const start = () => { setStatus(null); setApplying(true); };
  const finish = () => setApplying(false);
  const reset = useClientConfigurationResets({
    app,
    busy: busy || applying,
    canReset: recoveryReady,
    extraConfigurationCount: extraCount,
    onStart: start,
    onFinish: finish,
    onSuccess: (message) => { setStatus({ message, error: false }); onCommitted(); props.onApplied(); },
  });
  const prepareApply = () => {
    if (!settings || busy || applying) return;
    setPendingApply(null); setApplyError(null); start();
    void previewClientConfigurationApply(app, settings, subagentSettings).then((preview) => {
      setPendingApply({ preview, settings, subagentSettings });
    }).catch((error: { message?: string }) => {
      setApplyError(error.message ?? "无法生成配置预览");
    }).finally(finish);
  };
  const commitApply = () => {
    if (!pendingApply || busy || applying) return;
    setApplyError(null); start();
    void commitClientConfigurationApply(app, pendingApply.settings, pendingApply.preview, pendingApply.subagentSettings)
      .then(() => {
        setPendingApply(null);
        setStatus({ message: "已应用客户端通用配置。", error: false });
        onCommitted(); props.onApplied();
      })
      .catch((error: { message?: string }) => setApplyError(error.message ?? "应用客户端通用配置失败"))
      .finally(finish);
  };
  return {
    ...reset,
    applyError,
    applying,
    status,
    recoveryBlocker,
    extraConfigurationCount: extraCount,
    hasExtraConfiguration: extraCount > 0,
    canApply: fileIsWritable && !!settings && (app !== "codex" || !!subagentSettings),
    canNativeReset: recoveryReady,
    canClearExtra: recoveryReady && extraCount > 0,
    pendingApply,
    prepareApply,
    commitApply,
    cancelApply: () => setPendingApply(null),
  };
}

export type ClientConfigurationActionState = ReturnType<typeof useClientConfigurationApply>;

function ResetFailure({
  error,
  busy,
  onRetry,
  onBack,
}: {
  error: string;
  busy: boolean;
  onRetry: () => void;
  onBack?: () => void;
}) {
  return (
    <div className="asb-client-settings-reset-error" role="alert">
      <p className="asb-field-error">{error}</p>
      {onBack && <Button variant="secondary" disabled={busy} onClick={onBack}>返回恢复设置</Button>}
      <Button variant="secondary" disabled={busy} onClick={onRetry}>重新生成预览</Button>
    </div>
  );
}

function ExtraConfigurationAction({
  configuration,
  busy,
}: {
  configuration: ClientConfigurationActionState;
  busy: boolean;
}) {
  if (!configuration.hasExtraConfiguration) return null;
  return (
    <section className="asb-client-extra-configuration" aria-label="额外通用配置">
      <div>
        <h3 className="asb-section-title">额外通用配置</h3>
        <p className="asb-field-help">
          这些字段来自配置草稿，不包含在上方的原生默认值恢复中。当前管理 {configuration.extraConfigurationCount} 项额外配置。
        </p>
      </div>
      <Button
        variant="secondary"
        disabled={!configuration.canClearExtra || busy || configuration.applying}
        onClick={configuration.prepareExtraClear}
      >
        清空 ASB 管理的额外通用配置
      </Button>
    </section>
  );
}

export function ClientConfigurationResetPanel({
  app,
  busy,
  configuration,
}: {
  app: AppKind;
  busy: boolean;
  configuration: ClientConfigurationActionState;
}) {
  const working = busy || configuration.applying;
  if (configuration.resetView === "clearExtraConfiguration") {
    if (configuration.pendingExtraClear) {
      return (
        <ClientConfigurationResetPreview
          preview={configuration.pendingExtraClear.preview}
          app={app}
          resetKind="clearExtraConfiguration"
          extraConfigurationCount={configuration.extraConfigurationCount}
          busy={working}
          onConfirm={configuration.commitExtraClear}
          onBack={configuration.showNativeReset}
        />
      );
    }
    if (configuration.extraClearError) {
      return <ResetFailure error={configuration.extraClearError} busy={working} onRetry={configuration.prepareExtraClear} onBack={configuration.showNativeReset} />;
    }
    return <p className="asb-field-help" role="status">正在生成清空预览。</p>;
  }
  if (configuration.pendingNativeReset) {
    return <>
      <ClientConfigurationResetPreview
        preview={configuration.pendingNativeReset.preview}
        app={app}
        resetKind="nativeDefaults"
        extraConfigurationCount={configuration.extraConfigurationCount}
        busy={working}
        onConfirm={configuration.commitNativeReset}
      />
      <ExtraConfigurationAction configuration={configuration} busy={busy} />
    </>;
  }
  if (configuration.nativeResetError) {
    return <ResetFailure error={configuration.nativeResetError} busy={working} onRetry={configuration.prepareNativeReset} />;
  }
  return <p className="asb-field-help" role="status">正在生成恢复预览。</p>;
}

export function ClientConfigurationConfirmation({
  app,
  busy,
  applying,
  pending,
  onCancel,
  onConfirm,
}: {
  app: AppKind;
  busy: boolean;
  applying: boolean;
  pending: PendingClientConfiguration | null;
  onCancel: () => void;
  onConfirm: () => void;
}) {
  if (!pending) return null;
  return (
    <ConfirmSheet
      title="确认应用客户端通用配置"
      confirmLabel="确认应用"
      confirmDisabled={busy || applying}
      onConfirm={onConfirm}
      onCancel={onCancel}
    >
      <ul className="asb-dialog-details">
        <li>将写入 {pending.preview.file.preview.target}</li>
        <li><PreviewInspector filePreview={pending.preview.file} userConfigModel={null} userConfigWarnings={[]} /></li>
        <li>{app === "codex"
          ? "写入前会创建备份，并在同一可恢复事务中提交通用配置与子 agent 运行设置。"
          : "写入前会创建备份，并在同一可恢复事务中提交客户端通用配置。"}</li>
      </ul>
    </ConfirmSheet>
  );
}