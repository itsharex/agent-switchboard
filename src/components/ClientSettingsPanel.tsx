import { useId, useRef, useState, type ReactNode } from "react";
import { commitClientConfigurationApply, previewClientConfigurationApply, type AppKind, type ClientConfigurationApplyPreview, type CodexSubagentSettings, type SettingsValues, type SettingValue, type ConfigFileStatus } from "../api/client";
import { clientSettingsPayload } from "../app/claude-common-settings";
import type { ClientSettingsEditorState } from "../app/useClientSettings";
import { Button } from "./Button";
import { ClientPicker } from "./ClientPicker";
import { EditableCodePreview } from "./EditableCodePreview";
import { ConfirmSheet } from "./ConfirmSheet";
import { PreviewInspector } from "./PreviewInspector";
import { OfficialSettingsDirectory } from "./OfficialSettingsDirectory";
import { SettingsFields } from "./SettingsFields";

interface ClientSettingsPanelProps {
  app: AppKind;
  onSelectApp: (app: AppKind) => void;
  editorState: ClientSettingsEditorState;
  busy: boolean;
  configStatus: ConfigFileStatus | undefined;
  onValueChange: (app: AppKind, key: string, value: SettingValue) => void;
  onApplied: () => void;
  onRetryLoad: (app: AppKind) => void;
  onPreview: (app: AppKind) => void;
  onPreviewContentChange: (app: AppKind, content: string) => void;
  /** Codex's directly-applied subagent resource sits after model behavior,
   * outside the application-owned client-preference store. */
  subagentSettings?: ReactNode;
  subagentDraft?: CodexSubagentSettings;
}

function clientConfigStatus(
  configStatus: ConfigFileStatus | undefined,
): string | null {
  switch (configStatus?.matchStatus.kind) {
    case "matchesProfile":
      return `已应用：真实配置与「${configStatus.matchStatus.profileName}」一致`;
    case "externallyModified":
      return null;
    case "profileChanged":
      return `供应商「${configStatus.matchStatus.profileName}」或客户端设置已更新，请重新应用`;
    case "restoredBackup":
      return "真实配置已恢复备份，请前往供应商页重新应用";
    case "unmanaged":
      return "真实配置尚未由供应商应用管理";
    default:
      return null;
  }
}

function actionStatus(props: ClientSettingsPanelProps) {
  if (props.editorState.parseError) return { message: "客户端配置片段有错误，修正后才能预览", error: true };
  if (props.editorState.parsing) return { message: "正在同步客户端配置", error: false };
  if (props.editorState.phase === "dirty") return { message: "有未应用修改", error: false };
  const message = clientConfigStatus(props.configStatus);
  return message ? { message, error: props.configStatus?.matchStatus.kind === "externallyModified" } : null;
}

function ClientSettingsPreview({
  editorState: state,
  busy,
  app,
  onPreview,
  onPreviewContentChange,
}: ClientSettingsPanelProps) {
  const [open, setOpen] = useState(false);
  const previewId = useId();
  const visible = open && state.preview !== undefined;
  const label = state.previewing
    ? "正在生成配置编辑器"
    : visible
      ? "收起客户端配置编辑器"
      : state.preview
        ? "展开客户端配置编辑器"
        : "编辑客户端配置片段";
  return (
    <div className="asb-settings-preview">
      <div className="asb-form-actions">
        <Button
          variant="secondary"
          aria-expanded={visible}
          aria-controls={previewId}
          disabled={busy || state.previewing}
          onClick={() => {
            setOpen(!visible);
            // A draft edit invalidates the fragment in the state hook.
            if (!visible && !state.preview) onPreview(app);
          }}
        >
          {label}
        </Button>
      </div>
      {visible && state.preview && (
        <div id={previewId} role="region" aria-label="客户端配置编辑器">
          <EditableCodePreview
            target={state.preview.target}
            content={state.preview.content}
            disabled={busy}
            onChange={(content) => onPreviewContentChange(app, content)}
          />
        </div>
      )}
      {state.previewError && (
        <p className="asb-field-error" role="alert">
          无法生成客户端配置预览：{state.previewError.message}
        </p>
      )}
      {state.parseError && (
        <p className="asb-field-error" role="alert">
          客户端配置片段有错误：{state.parseError.message}
        </p>
      )}
    </div>
  );
}

function ClientPreferenceActions(props: ClientSettingsPanelProps) {
  const { editorState: state, app, busy } = props;
  const [pending, setPending] = useState<{
    preview: ClientConfigurationApplyPreview;
    settings: SettingsValues;
    subagentSettings?: CodexSubagentSettings;
  } | null>(null);
  const [applyError, setApplyError] = useState<string | null>(null);
  const [applying, setApplying] = useState(false);
  const settings: SettingsValues | null = state.editor && state.draft && !state.parsing && !state.parseError
    ? clientSettingsPayload(app, state.draft, state.claudeExtra) : null;
  const subagentSettings = app === "codex" ? props.subagentDraft : undefined;
  const defaults: SettingsValues | null = settings ? {
    settings: Object.fromEntries(Object.keys(settings.settings).map((key) => [key, { mode: "automatic" }])),
  } : null;
  const defaultSubagent: CodexSubagentSettings | undefined = app === "codex" ? {
    enabled: { mode: "automatic" },
    maxConcurrentThreadsPerSession: { mode: "automatic" },
    interruptMessage: { mode: "automatic" },
  } : undefined;
  const prepare = (nextSettings: SettingsValues, nextSubagent = subagentSettings) => {
    if (busy || applying) return;
    setApplying(true); setApplyError(null);
    void previewClientConfigurationApply(app, nextSettings, nextSubagent).then((preview) => {
      setPending({ preview, settings: nextSettings, subagentSettings: nextSubagent });
    }).catch((error: { message?: string }) => {
      setApplyError(error.message ?? "无法生成配置预览");
    }).finally(() => setApplying(false));
  };
  const status = actionStatus(props);
  return <>
    <section className="asb-client-configuration-actions" aria-label="配置操作">
      <div className="asb-client-configuration-reset">
        <Button variant="secondary" disabled={!defaults || busy || applying} onClick={() => defaults && prepare(defaults, defaultSubagent)}>
          {applying ? "正在生成预览" : "恢复通用配置默认值"}
        </Button>
      </div>
      <div className="asb-client-configuration-notice">
        {status && <span className={status.error ? "asb-field-error" : "asb-field-help"} role={status.error ? "alert" : "status"}>{status.message}</span>}
        {applyError && <p className="asb-field-error" role="alert">{applyError}</p>}
      </div>
      <div className="asb-client-configuration-commit">
        <Button variant="primary" disabled={!settings || (app === "codex" && !subagentSettings) || busy || applying}
          onClick={() => settings && prepare(settings)}>
          保存并预览应用
        </Button>
      </div>
    </section>
    {pending && <ConfirmSheet title="确认应用客户端通用配置" details={[
      `将写入 ${pending.preview.file.preview.target}`,
      <PreviewInspector filePreview={pending.preview.file} userConfigModel={null} userConfigWarnings={[]} />,
      app === "codex"
        ? "写入前会创建备份，并在同一可恢复事务中提交通用配置与子 agent 运行设置。"
        : "写入前会创建备份，并在同一可恢复事务中提交客户端通用配置。",
    ]} confirmLabel="确认应用" confirmDisabled={busy || applying} onConfirm={() => {
      if (busy || applying) return;
      setApplying(true); setApplyError(null);
      void commitClientConfigurationApply(app, pending.settings, pending.preview, pending.subagentSettings)
        .then(() => { setPending(null); props.onApplied(); })
        .catch((error: { message?: string }) => setApplyError(error.message ?? "应用客户端通用配置失败"))
        .finally(() => setApplying(false));
    }} onCancel={() => setPending(null)} />}
  </>;
}

function ClientPreferencesEditor(props: ClientSettingsPanelProps) {
  const { editorState: state, app, busy } = props;
  if (state.phase === "idle" || state.phase === "loading")
    return <>
      {props.subagentSettings}
      <p className="asb-empty">正在读取客户端设置</p>
    </>;
  if (state.phase === "loadError" || !state.editor || !state.draft) {
    return (
      <>
        {props.subagentSettings}
        <div className="asb-empty" role="alert">
          <p>
            无法读取客户端设置：{state.error?.message ?? "本地应用数据不可用"}
          </p>
          <Button
            variant="secondary"
            disabled={busy}
            onClick={() => props.onRetryLoad(app)}
          >
            重新读取
          </Button>
        </div>
      </>
    );
  }
  const working = busy;
  const modelBehaviorGroups = state.editor.groups.filter((group) => group === "模型行为");
  const remainingGroups = state.editor.groups.filter((group) => group !== "模型行为");
  return (
    <>
      {modelBehaviorGroups.length > 0 && (
        <SettingsFields
          specs={state.editor.specs}
          groups={modelBehaviorGroups}
          values={state.draft}
          baselineValues={state.editor.settings.settings}
          actualValues={props.configStatus ? (props.configStatus.clientSettings?.settings ?? {}) : undefined}
          busy={working}
          onChange={(key, value) => props.onValueChange(app, key, value)}
          showGroupReset={false}
        />
      )}
      {props.subagentSettings}
      {remainingGroups.length > 0 && (
        <SettingsFields
          specs={state.editor.specs}
          groups={remainingGroups}
          values={state.draft}
          baselineValues={state.editor.settings.settings}
          actualValues={props.configStatus ? (props.configStatus.clientSettings?.settings ?? {}) : undefined}
          busy={working}
          onChange={(key, value) => props.onValueChange(app, key, value)}
          showGroupReset={false}
        />
      )}
      <ClientPreferenceActions {...props} busy={working} />
      <ClientSettingsPreview {...props} busy={working} />
    </>
  );
}

export function ClientSettingsPanel(props: ClientSettingsPanelProps) {
  const [directoryOpen, setDirectoryOpen] = useState(false);
  const directoryId = useId();
  const helpButton = useRef<HTMLButtonElement>(null);
  const { app } = props;
  return (
    <div
      aria-label="客户端通用配置"
      onKeyDown={(event) => {
        if (event.key !== "Escape" || !directoryOpen) return;
        event.preventDefault();
        setDirectoryOpen(false);
        helpButton.current?.focus();
      }}
    >
      <div className="asb-client-preferences-toolbar">
        <ClientPicker
          app={app}
          disabled={props.busy}
          label="客户端通用配置客户端"
          onChange={(target) => {
            setDirectoryOpen(false);
            props.onSelectApp(target);
          }}
        />
        <Button
          ref={helpButton}
          variant="secondary"
          aria-expanded={directoryOpen}
          aria-controls={directoryId}
          disabled={!props.editorState.editor}
          onClick={() => setDirectoryOpen(!directoryOpen)}
        >
          {directoryOpen ? "返回客户端通用配置" : "官方设置目录"}
        </Button>
      </div>
      <div hidden={directoryOpen}>
        {props.configStatus?.clientSettingsError && (
          <p className="asb-field-error" role="alert">
            无法提取真实文件中的客户端设置：{props.configStatus.clientSettingsError}
          </p>
        )}
        <ClientPreferencesEditor {...props} />
      </div>
      {directoryOpen && (
        <div id={directoryId}>
          <OfficialSettingsDirectory
            app={app}
            entries={props.editorState.editor?.directory ?? []}
          />
        </div>
      )}
    </div>
  );
}
