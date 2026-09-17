import { useId, useRef, useState, type ReactNode } from "react";
import { type AppKind, type CodexSubagentSettings, type SettingValue, type ConfigFileStatus } from "../api/client";
import type { ClientSettingsEditorState } from "../app/useClientSettings";
import { Button } from "./Button";
import { ClientPicker } from "./ClientPicker";
import { CodePreview } from "./CodePreview";
import {
  ClientConfigurationConfirmation,
  ClientConfigurationResetPanel,
  type ClientConfigurationActionState,
  useClientConfigurationApply,
} from "./ClientConfigurationApply";
import { ClaudeExtraConfigurationEditor } from "./ClaudeExtraConfigurationEditor";
import { ClientManualConfigurationEditor } from "./ClientManualConfigurationEditor";
import { OfficialSettingsDirectory } from "./OfficialSettingsDirectory";
import { SettingsFields } from "./SettingsFields";

interface ClientSettingsPanelProps {
  app: AppKind;
  onSelectApp: (app: AppKind) => void;
  editorState: ClientSettingsEditorState;
  busy: boolean;
  configStatus: ConfigFileStatus | undefined;
  hasUnsavedConfigurationDraft: boolean;
  onValueChange: (app: AppKind, key: string, value: SettingValue) => void;
  onApplied: () => void;
  onRetryLoad: (app: AppKind) => void;
  onReview: (app: AppKind) => void;
  onClaudeExtraChange: (app: AppKind, extra: Record<string, unknown>) => void;
  globalInstructions: ReactNode;
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
  if (props.configStatus?.exists && !props.configStatus.syntaxOk) {
    return { message: "真实客户端配置格式错误，请在“配置审阅”中生成自动修复预览", error: true };
  }
  if (props.editorState.phase === "dirty") return { message: "有未应用修改", error: false };
  const message = clientConfigStatus(props.configStatus);
  return message ? { message, error: props.configStatus?.matchStatus.kind === "externallyModified" } : null;
}

interface ConfigurationReviewProps extends ClientSettingsPanelProps {
  open: boolean;
}

interface ConfigurationReviewButtonProps extends ConfigurationReviewProps {
  reviewId: string;
  onToggle: (trigger: HTMLButtonElement) => void;
}

function ClientConfigurationReviewButton({
  editorState: state,
  busy,
  open,
  reviewId,
  onToggle,
}: ConfigurationReviewButtonProps) {
  const label = state.currentConfigurationLoading
    ? "正在读取配置"
    : open ? "收起配置审阅" : "配置审阅";
  return (
    <Button
      variant="secondary"
      aria-expanded={open}
      aria-controls={reviewId}
      disabled={busy || state.currentConfigurationLoading}
      onClick={(event) => onToggle(event.currentTarget)}
    >
      {label}
    </Button>
  );
}

function ClientConfigurationReview({
  editorState: state,
  busy,
  app,
  onApplied,
  subagentDraft,
  onClaudeExtraChange,
}: ConfigurationReviewProps) {
  const source = state.currentConfiguration;
  if (!source && !state.currentConfigurationLoading && !state.currentConfigurationError) {
    return <p className="asb-field-help" role="status">正在准备配置审阅。</p>;
  }
  return (
    <div className="asb-client-configuration-review">
      <div className="asb-client-configuration-review-heading">
        <h3 className="asb-section-title">当前机器真实配置</h3>
        <p className="asb-field-help">此内容仅用于审阅事实，不能直接修改。</p>
      </div>
      {state.currentConfigurationLoading && <p className="asb-field-help" role="status">正在读取当前机器的真实配置。</p>}
      {state.currentConfigurationError && (
        <p className="asb-field-error" role="alert">
          无法读取当前机器的真实配置：{state.currentConfigurationError.message}
        </p>
      )}
      {source?.syntaxOk && (source.exists ? (
        <CodePreview target={`当前机器真实配置 · ${source.target}`} content={source.content} />
      ) : (
        <p className="asb-empty">当前机器尚未创建 {source.target}</p>
      ))}
      <section className="asb-client-configuration-review-scope" aria-label="ASB 管理范围">
        <h3 className="asb-section-title">ASB 管理范围</h3>
        <p className="asb-field-help">
          标准通用设置由本页表单管理；关闭配置审阅后，请使用对应设置项修改。
        </p>
        {app === "codex" && (
          <p className="asb-field-help">Codex 子 agent 的三项全局运行设置由“子 agent 运行”模块管理。</p>
        )}
      </section>
      {app === "claude" && (
        <ClaudeExtraConfigurationEditor
          extra={state.claudeExtra}
          busy={busy}
          onChange={(extra) => onClaudeExtraChange(app, extra)}
        />
      )}
      {source && (
        <ClientManualConfigurationEditor
          app={app}
          busy={busy}
          editorState={state}
          source={source}
          subagentDraft={subagentDraft}
          onApplied={onApplied}
        />
      )}
    </div>
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
          presentation="client"
          clientApp={app}
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
          presentation="client"
          clientApp={app}
        />
      )}
    </>
  );
}

interface ClientSettingsDisclosureProps {
  id: string;
  label: string;
  section: ClientSettingsSection;
  open: boolean;
  children: ReactNode;
}

function ClientSettingsDisclosure({ id, label, section, open, children }: ClientSettingsDisclosureProps) {
  return (
    <div
      id={id}
      role="region"
      aria-label={label}
      aria-hidden={!open}
      className={`asb-client-settings-disclosure is-${section}`}
      data-open={open}
    >
      <div className="asb-client-settings-disclosure-content">{children}</div>
    </div>
  );
}

type ClientSettingsSection = "directory" | "review" | "instructions" | "reset";

interface ClientSettingsToolbarProps {
  panel: ClientSettingsPanelProps;
  openSection: ClientSettingsSection | null;
  directoryId: string;
  reviewId: string;
  instructionsId: string;
  resetId: string;
  onSectionChange: (section: ClientSettingsSection | null, trigger: HTMLButtonElement) => void;
  resetDisabled: boolean;
  resetLabel: string;
  onReset: (trigger: HTMLButtonElement) => void;
}

function ClientSettingsToolbar({
  panel,
  openSection,
  directoryId,
  reviewId,
  instructionsId,
  resetId,
  onSectionChange,
  resetDisabled,
  resetLabel,
  onReset,
}: ClientSettingsToolbarProps) {
  const directoryOpen = openSection === "directory";
  const reviewOpen = openSection === "review";
  const instructionsOpen = openSection === "instructions";
  const resetOpen = openSection === "reset";
  return (
    <div className="asb-client-preferences-toolbar">
      <ClientPicker
        app={panel.app}
        disabled={panel.busy}
        label="客户端通用配置客户端"
        onChange={(target) => panel.onSelectApp(target)}
      />
      <div className="asb-client-preferences-toolbar-actions">
        <Button
          variant="secondary"
          aria-expanded={resetOpen}
          aria-controls={resetId}
          disabled={resetDisabled}
          onClick={(event) => onReset(event.currentTarget)}
        >
          {resetLabel}
        </Button>
        <Button
          variant="secondary"
          aria-expanded={directoryOpen}
          aria-controls={directoryId}
          disabled={!panel.editorState.editor}
          onClick={(event) => onSectionChange(directoryOpen ? null : "directory", event.currentTarget)}
        >
          {directoryOpen ? "返回客户端通用配置" : "官方设置目录"}
        </Button>
        <ClientConfigurationReviewButton
          {...panel}
          open={reviewOpen}
          reviewId={reviewId}
          onToggle={(trigger) => {
            const next = !reviewOpen;
            onSectionChange(next ? "review" : null, trigger);
            if (next) panel.onReview(panel.app);
          }}
        />
        <Button
          variant="secondary"
          aria-expanded={instructionsOpen}
          aria-controls={instructionsId}
          disabled={panel.busy}
          onClick={(event) => onSectionChange(instructionsOpen ? null : "instructions", event.currentTarget)}
        >
          {instructionsOpen ? "收起全局指令" : "全局指令"}
        </Button>
      </div>
    </div>
  );
}

function ClientSettingsForm({
  panel,
  directoryOpen,
  reviewOpen,
  resetOpen,
  configuration,
}: {
  panel: ClientSettingsPanelProps;
  directoryOpen: boolean;
  reviewOpen: boolean;
  resetOpen: boolean;
  configuration: ClientConfigurationActionState;
}) {
  const status = configuration.status ?? actionStatus(panel);
  return <div hidden={directoryOpen || reviewOpen || resetOpen}>
    {panel.configStatus?.clientSettingsError && (
      <p className="asb-field-error" role="alert">
        无法提取真实文件中的客户端设置：{panel.configStatus.clientSettingsError}
      </p>
    )}
    <ClientPreferencesEditor {...panel} />
    <section className="asb-client-configuration-actions" aria-label="配置操作">
      <div className="asb-client-configuration-notice">
        {status && <span className={status.error ? "asb-field-error" : "asb-field-help"} role={status.error ? "alert" : "status"}>{status.message}</span>}
        {configuration.recoveryBlocker && <p className="asb-field-help">{configuration.recoveryBlocker}</p>}
        {configuration.applyError && <p className="asb-field-error" role="alert">{configuration.applyError}</p>}
      </div>
      <div className="asb-client-configuration-commit">
        <Button variant="primary" disabled={!configuration.canApply || panel.busy || configuration.applying}
          onClick={configuration.prepareApply}>
          保存并预览应用
        </Button>
      </div>
    </section>
  </div>;
}

export function ClientSettingsPanel(props: ClientSettingsPanelProps) {
  const [openSection, setOpenSection] = useState<ClientSettingsSection | null>(null);
  const directoryId = useId();
  const reviewId = useId();
  const instructionsId = useId();
  const resetId = useId();
  const panelTrigger = useRef<HTMLButtonElement>(null);
  const directoryOpen = openSection === "directory";
  const reviewOpen = openSection === "review";
  const instructionsOpen = openSection === "instructions";
  const resetOpen = openSection === "reset";
  const changeSection = (section: ClientSettingsSection | null, trigger: HTMLButtonElement) => {
    panelTrigger.current = trigger;
    setOpenSection(section);
  };
  const configuration = useClientConfigurationApply(props, () => setOpenSection(null));
  return <>
    <div className="asb-client-settings-panel" aria-label="客户端通用配置" onKeyDown={(event) => {
      if (event.key !== "Escape" || !openSection) return;
      event.preventDefault(); setOpenSection(null); panelTrigger.current?.focus();
    }}>
      <ClientSettingsToolbar
        panel={props}
        openSection={openSection}
        directoryId={directoryId}
        reviewId={reviewId}
        instructionsId={instructionsId}
        resetId={resetId}
        onSectionChange={changeSection}
        resetDisabled={!configuration.canNativeReset || props.busy || configuration.applying}
        resetLabel={resetOpen && configuration.applying
          ? configuration.resetView === "clearExtraConfiguration" ? "正在生成清空预览" : "正在生成恢复预览"
          : resetOpen ? "收起恢复设置" : "恢复为客户端原生默认值"}
        onReset={(trigger) => {
          const next = !resetOpen;
          changeSection(next ? "reset" : null, trigger);
          if (next) configuration.prepareNativeReset();
        }}
      />
      <ClientSettingsDisclosure id={reviewId} label="配置审阅" section="review" open={reviewOpen}>
        <ClientConfigurationReview {...props} open={reviewOpen} />
      </ClientSettingsDisclosure>
      <ClientSettingsDisclosure id={instructionsId} label="全局指令编辑器" section="instructions" open={instructionsOpen}>
        {props.globalInstructions}
      </ClientSettingsDisclosure>
      <ClientSettingsDisclosure id={directoryId} label="官方设置目录" section="directory" open={directoryOpen}>
        <OfficialSettingsDirectory entries={props.editorState.editor?.directory ?? []} />
      </ClientSettingsDisclosure>
      <ClientSettingsDisclosure id={resetId} label="恢复为客户端原生默认值" section="reset" open={resetOpen}>
        <ClientConfigurationResetPanel app={props.app} busy={props.busy} configuration={configuration} />
      </ClientSettingsDisclosure>
      <ClientSettingsForm panel={props} directoryOpen={directoryOpen} reviewOpen={reviewOpen} resetOpen={resetOpen} configuration={configuration} />
    </div>
    <ClientConfigurationConfirmation
      app={props.app}
      busy={props.busy}
      applying={configuration.applying}
      pending={configuration.pendingApply}
      onConfirm={configuration.commitApply}
      onCancel={configuration.cancelApply}
    />
  </>;
}
