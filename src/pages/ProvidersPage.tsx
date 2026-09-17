import type {
  AppKind,
  ConfigFileStatus,
  LockStatus,
  ProviderDraft,
  ProviderProfile,
  UsageQuery,
} from "../api/client";
import type { ProviderView } from "../app/navigation";
import { ProviderEditor } from "../components/ProviderEditor";
import { ProviderList } from "../components/ProviderList";
import { ProviderWorkspaceShell } from "../components/ProviderWorkspaceShell";
import { UsageQueryWorkspace } from "../components/UsageQueryWorkspace";
import type { ProviderEditorSession } from "../app/useProviders";

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
  /** The generic editor serves Claude only; Codex owns its specialized editor. */
  editorSession: Extract<ProviderEditorSession, { app: "claude" }> | null;
  busy: boolean;
  /** Persisted profile ids whose usage panel is collapsed. */
  collapsedUsageIds: string[];
  onSelectApp: (app: AppKind) => void;
  onNew: () => void;
  onImport: () => void;
  onCloseEditor: () => void;
  onSave: (draft: ProviderDraft) => Promise<void>;
  /** Replaces the editor session when the user picks Codex from the editor. */
  onSwitchClient: (app: AppKind) => void;
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
  onReorder: (orderedIds: string[]) => void;
  /** Persists the flipped usage-panel state for the profile. */
  onToggleUsage: (profile: ProviderProfile) => void;
  onActivate: (profile: ProviderProfile) => void;
  onEdit: (profile: ProviderProfile) => void;
  onDelete: (profile: ProviderProfile) => void;
  /** Refreshes the provider snapshot after a management-dialog change. */
  onRefresh?: () => Promise<void>;
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
      onSwitchClient={props.onSwitchClient}
      onCancel={props.onCloseEditor}
    />
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
    <ProviderWorkspaceShell
      ariaLabel="供应商工作区"
      app={appFilter}
      onSelectApp={props.onSelectApp}
      busy={busy}
      statuses={props.statuses}
      profiles={props.profiles}
      locks={props.locks}
      onImport={props.onImport}
      onNew={props.onNew}
    >
      <ProviderList
        profiles={props.profiles.filter((profile) => profile.app === appFilter)}
        activeProfileId={props.activeProfileId}
        userConfigModel={props.userConfigModel}
        collapsedUsageIds={props.collapsedUsageIds}
        onReorder={props.onReorder}
        onToggleUsage={props.onToggleUsage}
        onSaveQuotaInterval={props.onSaveQuotaInterval}
        onActivate={props.onActivate}
        onEdit={props.onEdit}
        onConfigureUsage={onConfigureUsage}
        onDelete={props.onDelete}
      />
    </ProviderWorkspaceShell>
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
            authentication={usageProfile.authentication}
            connection={usageProfile.connection}
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
      <div className="asb-provider-editor-route" hidden={!props.active}>
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
