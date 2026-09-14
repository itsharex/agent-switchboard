import { useId, useRef, useState, type ReactNode } from "react";
import type { AppKind, SettingValue, ConfigFileStatus } from "../api/client";
import type { ClientSettingsEditorState } from "../app/useClientSettings";
import { Button } from "./Button";
import { ClientPicker } from "./ClientPicker";
import { EditableCodePreview } from "./EditableCodePreview";
import { OfficialSettingsDirectory } from "./OfficialSettingsDirectory";
import { SettingsFields } from "./SettingsFields";

interface ClientSettingsPanelProps {
  app: AppKind;
  onSelectApp: (app: AppKind) => void;
  editorState: ClientSettingsEditorState;
  busy: boolean;
  configStatus: ConfigFileStatus | undefined;
  hasActiveProvider: boolean;
  previewBlockedReason: string | null;
  onValueChange: (app: AppKind, key: string, value: SettingValue) => void;
  onResetGroup: (app: AppKind, group: string | null) => void;
  onSave: (app: AppKind) => void;
  onSaveAndPreview: (app: AppKind) => void;
  onOpenProviders: () => void;
  onRetryLoad: (app: AppKind) => void;
  onPreview: (app: AppKind) => void;
  onPreviewContentChange: (app: AppKind, content: string) => void;
  /** Codex's directly-applied subagent resource sits after model behavior,
   * outside the application-owned client-preference store. */
  subagentSettings?: ReactNode;
}

function clientConfigStatus(
  configStatus: ConfigFileStatus | undefined,
): string | null {
  switch (configStatus?.matchStatus.kind) {
    case "matchesProfile":
      return `已应用：真实配置与「${configStatus.matchStatus.profileName}」一致`;
    case "externallyModified":
      return "真实配置已被外部修改，请前往供应商页重新应用";
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
  if (props.editorState.parseError) {
    return { message: "客户端配置片段有错误，修正后才能保存", error: true };
  }
  if (props.editorState.parsing) {
    return { message: "正在同步客户端配置", error: false };
  }
  if (
    props.editorState.phase === "dirty" ||
    props.editorState.phase === "saveError"
  ) {
    return { message: "有未保存修改", error: false };
  }
  if (
    props.editorState.phase === "savedPendingReapply" &&
    props.configStatus?.matchStatus.kind !== "matchesProfile"
  ) {
    return {
      message: props.hasActiveProvider
        ? "已保存，预览并确认应用后生效"
        : "已保存，请在供应商页选择并启用供应商后生效",
      error: false,
    };
  }
  const message = clientConfigStatus(props.configStatus);
  if (message)
    return {
      message,
      error: props.configStatus?.matchStatus.kind === "externallyModified",
    };
  return props.editorState.phase === "clean"
    ? { message: "已保存到应用数据", error: false }
    : null;
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
            disabled={busy || state.phase === "saving"}
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
  const { editorState: state, app, busy, hasActiveProvider } = props;
  const status = actionStatus(props);
  const canSave = !state.parsing && !state.parseError &&
    (state.phase === "dirty" || state.phase === "saveError");
  const providerHelpId = useId();
  const previewHelp = !hasActiveProvider
    ? "当前客户端没有已启用的供应商；请前往供应商页选择并启用。"
    : props.previewBlockedReason;
  return (
    <>
      <div className="asb-settings-actions">
        <Button
          variant="secondary"
          disabled={busy}
          onClick={() => props.onResetGroup(app, null)}
        >
          全部恢复默认值
        </Button>
        <div className="asb-settings-actions-main">
          {status && (
            <span
              className={status.error ? "asb-field-error" : "asb-field-help"}
              role={status.error ? "alert" : "status"}
            >
              {status.message}
            </span>
          )}
          <div className="asb-client-preference-save">
            <Button
              variant="secondary"
              disabled={busy || !canSave}
              onClick={() => props.onSave(app)}
            >
              {state.phase === "saving" ? "正在保存" : "保存客户端设置"}
            </Button>
            <Button
              variant="primary"
              disabled={busy || state.parsing || Boolean(state.parseError) || previewHelp !== null}
              aria-describedby={previewHelp ? providerHelpId : undefined}
              onClick={() => props.onSaveAndPreview(app)}
            >
              保存并预览应用
            </Button>
          </div>
        </div>
      </div>
      {previewHelp && (
        <div className="asb-client-provider-help">
          <p id={providerHelpId} className="asb-field-help">
            {previewHelp}
          </p>
          <Button
            variant="secondary"
            disabled={busy}
            onClick={props.onOpenProviders}
          >
            前往供应商
          </Button>
        </div>
      )}
    </>
  );
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
  const working = busy || state.phase === "saving";
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
          busy={working}
          onChange={(key, value) => props.onValueChange(app, key, value)}
          onResetGroup={(group) => props.onResetGroup(app, group)}
        />
      )}
      {props.subagentSettings}
      {remainingGroups.length > 0 && (
        <SettingsFields
          specs={state.editor.specs}
          groups={remainingGroups}
          values={state.draft}
          baselineValues={state.editor.settings.settings}
          busy={working}
          onChange={(key, value) => props.onValueChange(app, key, value)}
          onResetGroup={(group) => props.onResetGroup(app, group)}
        />
      )}
      <ClientPreferenceActions {...props} busy={working} />
      {state.phase === "saveError" && (
        <p className="asb-field-error" role="alert">
          保存失败，修改已保留：{state.error?.message ?? "请重试"}
        </p>
      )}
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
      aria-label="偏好设置"
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
          label="偏好设置客户端"
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
          {directoryOpen ? "返回偏好设置" : "官方设置目录"}
        </Button>
      </div>
      <div hidden={directoryOpen}>
        <p className="asb-field-help asb-client-preferences-note">
          这里集中记录所选客户端的共享偏好，方便独立调整。保存不会修改、切换或覆盖任何供应商；只有随后预览并确认应用时，才会把当前供应商与这些偏好一起写入客户端配置。
        </p>
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
