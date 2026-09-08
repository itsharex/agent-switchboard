import type {
  AppKind,
  ConfigFileStatus,
  FilePreview,
  LockStatus,
  ProviderDraft,
  ProviderProfile,
  UsageQuery,
} from "../api/client";
import type { ProviderView } from "../app/navigation";
import { Button } from "../components/Button";
import { ClientPicker } from "../components/ClientPicker";
import { DualRelay } from "../components/DualRelay";
import { PlusIcon } from "../components/icons";
import { PreviewInspector } from "../components/PreviewInspector";
import { ProviderEditor } from "../components/ProviderEditor";
import { ProviderList } from "../components/ProviderList";
import { UsageQueryWorkspace } from "../components/UsageQueryWorkspace";
import type { ProviderEditorSession } from "../app/useProviders";
import type { DiagnosticSection } from "../app/navigation";
import "../styles/base/provider-workspace.css";

interface ProvidersPageProps {
  view: ProviderView;
  onViewChange: (view: ProviderView) => void;
  active: boolean;
  profiles: ProviderProfile[];
  appFilter: AppKind;
  activeProfileId: string | null;
  statuses: ConfigFileStatus[] | null;
  locks: Partial<Record<AppKind, LockStatus>>;
  /** Model read from this client's user-level configuration file. */
  userConfigModel: string | null;
  /** Known conditions that can override the user-level configuration. */
  userConfigWarnings: string[];
  selectedId: string | null;
  editorSession: ProviderEditorSession | null;
  preview: { profileId: string; file: FilePreview } | null;
  busy: boolean;
  /** Persisted profile ids whose usage panel is collapsed. */
  collapsedUsageIds: string[];
  onSelectApp: (app: AppKind) => void;
  onNew: () => void;
  onImport: () => void;
  onOpenClientSettings: () => void;
  onOpenHistory: () => void;
  onOpenDiagnostics: (section: DiagnosticSection) => void;
  onOpenQuota: () => void;
  onCloseEditor: () => void;
  onSave: (draft: ProviderDraft) => Promise<void>;
  onSaveUsageQuery: (
    profile: ProviderProfile,
    usageQuery: UsageQuery | null,
  ) => Promise<boolean>;
  /** Persists the official Codex quota panel's auto-refresh interval for the
   * profile; 0 turns the scheduled query off. */
  onSaveQuotaInterval: (
    profile: ProviderProfile,
    minutes: number,
  ) => Promise<boolean>;
  onSelect: (profileId: string) => void;
  onReorder: (orderedIds: string[]) => void;
  /** Persists the flipped usage-panel state for the profile. */
  onToggleUsage: (profile: ProviderProfile) => void;
  onActivate: (profile: ProviderProfile) => void;
  onTogglePreview: (profile: ProviderProfile) => void;
  onEdit: (profile: ProviderProfile) => void;
  onDelete: (profile: ProviderProfile) => void;
  onRequestSwitch: () => void;
  onCancelPreview: () => void;
}

function ProviderEditView(props: ProvidersPageProps) {
  const { profiles, editorSession, busy } = props;
  if (!editorSession) return null;
  const profile = editorSession.record?.profile ?? null;
  return (
    <ProviderEditor
      key={profile?.id ?? `new-${editorSession.app}`}
      active={props.active}
      profile={profile}
      initialApp={editorSession.app}
      busy={busy}
      officialTakenApps={profiles
        .filter((profile) => profile.routeMode === "official")
        .map((profile) => profile.app)}
      onOpenOfficial={(app) => {
        const official = profiles.find(
          (profile) => profile.app === app && profile.routeMode === "official",
        );
        if (official) {
          props.onSelectApp(app);
          props.onEdit(official);
        }
      }}
      userConfigModel={props.userConfigModel}
      userConfigWarnings={props.userConfigWarnings}
      onSave={props.onSave}
      onCancel={props.onCloseEditor}
    />
  );
}

function ProviderPreview(props: ProvidersPageProps) {
  if (!props.preview) return null;
  return (
    <section className="asb-preview-inline" aria-label="变更预览">
      <div className="asb-panel-heading">
        <h3 className="asb-panel-title">变更预览</h3>
        <div className="asb-panel-actions">
          <Button
            variant="secondary"
            disabled={props.busy}
            onClick={props.onCancelPreview}
          >
            取消
          </Button>
          <Button
            variant="primary"
            disabled={props.busy}
            onClick={props.onRequestSwitch}
          >
            确认切换
          </Button>
        </div>
      </div>
      <PreviewInspector
        filePreview={props.preview.file}
        userConfigModel={props.userConfigModel}
        userConfigWarnings={props.userConfigWarnings}
      />
    </section>
  );
}

function ProviderListView({
  onConfigureUsage,
  ...props
}: ProvidersPageProps & {
  onConfigureUsage: (profile: ProviderProfile) => void;
}) {
  const { appFilter, busy } = props;
  return (
    <section
      className="asb-panel asb-provider-workspace"
      aria-label="供应商工作区"
    >
      <div className="asb-panel-heading">
        <h2 className="asb-panel-title">供应商</h2>
        <div className="asb-provider-toolbar">
          <Button variant="secondary" onClick={props.onOpenClientSettings}>
            偏好设置
          </Button>
          <Button variant="secondary" onClick={props.onOpenHistory}>
            切换历史
          </Button>
        </div>
      </div>
      <DualRelay
        statuses={props.statuses}
        profiles={props.profiles}
        locks={props.locks}
        onOpenDiagnostics={props.onOpenDiagnostics}
        onOpenQuota={props.onOpenQuota}
      />
      <div className="asb-tabs-bar">
        <ClientPicker
          app={appFilter}
          onChange={props.onSelectApp}
          disabled={busy}
          label="供应商客户端"
        />
        <div className="asb-provider-toolbar">
          <Button variant="secondary" disabled={busy} onClick={props.onImport}>
            导入
          </Button>
          <Button variant="plus" disabled={busy} onClick={props.onNew}>
            <PlusIcon />
            新建供应商
          </Button>
        </div>
      </div>
      <ProviderList
        profiles={props.profiles.filter((profile) => profile.app === appFilter)}
        activeProfileId={props.activeProfileId}
        userConfigModel={props.userConfigModel}
        selectedId={props.selectedId}
        openPreviewId={props.preview?.profileId ?? null}
        collapsedUsageIds={props.collapsedUsageIds}
        onSelect={props.onSelect}
        onReorder={props.onReorder}
        onToggleUsage={props.onToggleUsage}
        onSaveQuotaInterval={props.onSaveQuotaInterval}
        onActivate={props.onActivate}
        onPreview={props.onTogglePreview}
        onEdit={props.onEdit}
        onConfigureUsage={onConfigureUsage}
        onDelete={props.onDelete}
        renderPreview={() => <ProviderPreview {...props} />}
      />
    </section>
  );
}

/** Editors retain their drafts while inactive; only the visible list mounts polling cards. */
export function ProvidersPage(props: ProvidersPageProps) {
  const usageProfile = props.view.kind === "usage" ? props.view.profile : null;
  if (usageProfile) {
    return (
      <div className="asb-edit-view" hidden={!props.active}>
        <section className="asb-panel asb-edit-panel">
          <UsageQueryWorkspace
            key={usageProfile.id}
            providerName={usageProfile.name}
            value={usageProfile.usageQuery ?? null}
            apiKey={usageProfile.apiKey}
            baseUrl={usageProfile.baseUrl}
            upstreamProtocol={usageProfile.upstreamProtocol}
            busy={props.busy}
            onSave={async (usageQuery) => {
              const saved = await props.onSaveUsageQuery(
                usageProfile,
                usageQuery,
              );
              if (saved) props.onViewChange({ kind: "list" });
              return saved;
            }}
            onClose={() => props.onViewChange({ kind: "list" })}
          />
        </section>
      </div>
    );
  }
  if (props.editorSession !== null) {
    return (
      <div hidden={!props.active}>
        <ProviderEditView {...props} />
      </div>
    );
  }
  return props.active ? (
    <ProviderListView
      {...props}
      onConfigureUsage={(profile) =>
        props.onViewChange({ kind: "usage", profile })
      }
    />
  ) : null;
}
