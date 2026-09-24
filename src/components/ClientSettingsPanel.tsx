import { commandErrorText } from "../i18n/errors";
import { useId, useRef, useState, type ReactNode } from "react";
import { type AppKind, type CodexSubagentSettings, type SettingValue, type ConfigFileStatus } from "../api/client";
import type { ClientSettingsEditorState } from "../app/useClientSettings";
import { useI18n, type TFunction } from "../i18n";
import { Button } from "./Button";
import { ClientPicker } from "./ClientPicker";
import { CurrentConfigurationEditor } from "./CurrentConfigurationEditor";
import {
  ClientConfigurationConfirmation,
  ClientConfigurationResetPanel,
  type ClientConfigurationActionState,
  useClientConfigurationApply,
} from "./ClientConfigurationApply";
import { ClaudeExtraConfigurationEditor } from "./ClaudeExtraConfigurationEditor";
import { OfficialSettingsDirectory } from "./OfficialSettingsDirectory";
import { SettingsFields } from "./SettingsFields";
import { WorkspaceHeader } from "./WorkspaceHeader";

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
  t: TFunction,
  configStatus: ConfigFileStatus | undefined,
): string | null {
  switch (configStatus?.matchStatus.kind) {
    case "matchesProfile":
      return t("clientConfig.status.matchesProfile", { name: configStatus.matchStatus.profileName });
    case "externallyModified":
      return null;
    case "profileChanged":
      return t("clientConfig.status.profileChanged", { name: configStatus.matchStatus.profileName });
    case "restoredBackup":
      return t("clientConfig.status.restoredBackup");
    case "unmanaged":
      return t("clientConfig.status.unmanaged");
    default:
      return null;
  }
}

function actionStatus(t: TFunction, props: ClientSettingsPanelProps) {
  if (props.configStatus?.exists && !props.configStatus.syntaxOk) {
    return { message: t("clientConfig.status.syntaxError"), error: true };
  }
  if (props.editorState.phase === "dirty") return { message: t("clientConfig.status.dirty"), error: false };
  const message = clientConfigStatus(t, props.configStatus);
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
  const { t } = useI18n();
  const label = state.currentConfigurationLoading
    ? t("clientConfig.review.reading")
    : open ? t("clientConfig.review.collapse") : t("clientConfig.review.title");
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
  const { t } = useI18n();
  const source = state.currentConfiguration;
  if (!source && !state.currentConfigurationLoading && !state.currentConfigurationError) {
    return <p className="asb-field-help" role="status">{t("clientConfig.review.preparing")}</p>;
  }
  return (
    <div className="asb-client-configuration-review">
      {state.currentConfigurationLoading && <p className="asb-field-help" role="status">{t("clientConfig.review.readingReal")}</p>}
      {state.currentConfigurationError && (
        <p className="asb-field-error" role="alert">
          {t("clientConfig.review.readError", { message: commandErrorText(state.currentConfigurationError, t) })}
        </p>
      )}
      {source && (
        <CurrentConfigurationEditor
          app={app}
          busy={busy}
          editorState={state}
          source={source}
          subagentDraft={subagentDraft}
          onApplied={onApplied}
        />
      )}
      <section className="asb-client-configuration-review-scope" aria-label={t("clientConfig.review.scopeTitle")}>
        <h3 className="asb-section-title">{t("clientConfig.review.scopeTitle")}</h3>
        <p className="asb-field-help">
          {t("clientConfig.review.scopeHelp")}
        </p>
        {app === "codex" && (
          <p className="asb-field-help">{t("clientConfig.review.scopeCodexHelp")}</p>
        )}
      </section>
      {app === "claude" && (
        <ClaudeExtraConfigurationEditor
          extra={state.claudeExtra}
          busy={busy}
          onChange={(extra) => onClaudeExtraChange(app, extra)}
        />
      )}
    </div>
  );
}
function ClientPreferencesEditor(props: ClientSettingsPanelProps) {
  const { editorState: state, app, busy } = props;
  const { t } = useI18n();
  if (state.phase === "idle" || state.phase === "loading")
    return <>
      {props.subagentSettings}
      <div className="asb-settings-skeleton" role="status" aria-label={t("clientConfig.loading")}>
        <div className="asb-skeleton" />
        <div className="asb-skeleton" />
        <div className="asb-skeleton" />
      </div>
    </>;
  if (state.phase === "loadError" || !state.editor || !state.draft) {
    return (
      <>
        {props.subagentSettings}
        <div className="asb-empty" role="alert">
          <p>
            {t("clientConfig.loadError", { message: state.error ? commandErrorText(state.error, t) : t("clientConfig.loadErrorFallback") })}
          </p>
          <Button
            variant="secondary"
            disabled={busy}
            onClick={() => props.onRetryLoad(app)}
          >
            {t("clientConfig.common.reload")}
          </Button>
        </div>
      </>
    );
  }
  const working = busy;
  // The backend catalog emits stable group keys; the display name resolves
  // through the catalog at render time.
  const modelBehaviorGroups = state.editor.groups.filter((group) => group === "ownership.group.modelBehavior");
  const remainingGroups = state.editor.groups.filter((group) => group !== "ownership.group.modelBehavior");
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
  configuration: ClientConfigurationActionState;
  onSectionChange: (section: ClientSettingsSection | null, trigger: HTMLButtonElement) => void;
}

/** The page header: the client picker leads the navigation row, the four
 * disclosure entries sit right of it like every workspace's page actions. */
function ClientSettingsToolbar({
  panel,
  openSection,
  directoryId,
  reviewId,
  instructionsId,
  resetId,
  configuration,
  onSectionChange,
}: ClientSettingsToolbarProps) {
  const { t } = useI18n();
  const directoryOpen = openSection === "directory";
  const reviewOpen = openSection === "review";
  const instructionsOpen = openSection === "instructions";
  const resetOpen = openSection === "reset";
  return (
    <WorkspaceHeader
      title={t("clientConfig.title")}
      primary={
        <ClientPicker
          app={panel.app}
          disabled={panel.busy}
          label={t("clientConfig.pickerLabel")}
          onChange={(target) => panel.onSelectApp(target)}
        />
      }
      primaryActions={
        <>
          <Button
            variant="secondary"
            aria-expanded={directoryOpen}
            aria-controls={directoryId}
            disabled={!panel.editorState.editor}
            onClick={(event) => onSectionChange(directoryOpen ? null : "directory", event.currentTarget)}
          >
            {directoryOpen ? t("clientConfig.toolbar.backFromDirectory") : t("clientConfig.toolbar.directory")}
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
            {instructionsOpen ? t("clientConfig.toolbar.collapseInstructions") : t("clientConfig.toolbar.instructions")}
          </Button>
          <Button
            variant="danger"
            aria-expanded={resetOpen}
            aria-controls={resetId}
            disabled={!configuration.canNativeReset || panel.busy || configuration.applying}
            onClick={(event) => {
              const next = !resetOpen;
              onSectionChange(next ? "reset" : null, event.currentTarget);
              if (next) configuration.prepareNativeReset("nativeDefaults");
            }}
          >
            {resetOpen && configuration.applying
              ? configuration.resetView === "clearExtraConfiguration" ? t("clientConfig.toolbar.generatingClear") : t("clientConfig.toolbar.generatingNative")
              : resetOpen ? t("clientConfig.toolbar.collapseReset") : t("clientConfig.reset.nativeTitle")}
          </Button>
        </>
      }
    />
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
  const { t } = useI18n();
  const status = configuration.status ?? actionStatus(t, panel);
  return <div hidden={directoryOpen || reviewOpen || resetOpen}>
    {panel.configStatus?.clientSettingsError && (
      <p className="asb-field-error" role="alert">
        {t("clientConfig.status.extractError", { message: panel.configStatus.clientSettingsError })}
      </p>
    )}
    <ClientPreferencesEditor {...panel} />
    <section className="asb-client-configuration-actions" aria-label={t("clientConfig.form.actionsAria")}>
      <div className="asb-client-configuration-notice">
        {status && <span className={status.error ? "asb-field-error" : "asb-field-help"} role={status.error ? "alert" : "status"}>{status.message}</span>}
        {configuration.recoveryBlocker && <p className="asb-field-help">{configuration.recoveryBlocker}</p>}
        {configuration.applyError && <p className="asb-field-error" role="alert">{configuration.applyError}</p>}
      </div>
      <div className="asb-client-configuration-commit">
        <Button variant="primary" disabled={!configuration.canApply || panel.busy || configuration.applying}
          onClick={configuration.prepareApply}>
          {t("clientConfig.apply.saveAndPreview")}
        </Button>
      </div>
    </section>
  </div>;
}

export function ClientSettingsPanel(props: ClientSettingsPanelProps) {
  const { t } = useI18n();
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
    <div className="asb-client-settings-panel" aria-label={t("clientConfig.title")} onKeyDown={(event) => {
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
        configuration={configuration}
        onSectionChange={changeSection}
      />
      <ClientSettingsDisclosure id={reviewId} label={t("clientConfig.review.title")} section="review" open={reviewOpen}>
        <ClientConfigurationReview {...props} open={reviewOpen} />
      </ClientSettingsDisclosure>
      <ClientSettingsDisclosure id={instructionsId} label={t("clientConfig.toolbar.instructionsEditor")} section="instructions" open={instructionsOpen}>
        {props.globalInstructions}
      </ClientSettingsDisclosure>
      <ClientSettingsDisclosure id={directoryId} label={t("clientConfig.toolbar.directory")} section="directory" open={directoryOpen}>
        <OfficialSettingsDirectory entries={props.editorState.editor?.directory ?? []} />
      </ClientSettingsDisclosure>
      <ClientSettingsDisclosure id={resetId} label={t("clientConfig.reset.nativeTitle")} section="reset" open={resetOpen}>
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
