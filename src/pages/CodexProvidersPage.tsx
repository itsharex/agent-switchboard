import { useCallback, useEffect, useRef, useState } from "react";
import {
  executeSwitch,
  previewSwitch,
  reorderCodexProfiles,
  type AppKind,
  type CodexProviderDraft,
  type CodexProviderRecord,
  type CommandError,
  type ConfigFileStatus,
  type FilePreview,
  type LockStatus,
  type ProviderDraft,
  type ProviderProfile,
  type ProviderRecord,
} from "../api/client";
import { openUrl } from "@tauri-apps/plugin-opener";
import type { CodexEditorSource } from "../app/useProviders";
import { Button } from "../components/Button";
import { CodexOfficialQuotaPanel } from "../components/CodexOfficialQuotaPanel";
import { CodexProviderEditor } from "../components/codex-provider-editor/CodexProviderEditor";
import { EditIcon, EyeOffIcon, PlayIcon, PreviewIcon, UsageIcon } from "../components/icons";
import { OfficialLoginPanel } from "../components/OfficialLoginPanel";
import { PreviewInspector } from "../components/PreviewInspector";
import { ProviderMoreActions } from "../components/ProviderMoreActions";
import {
  ProviderRowShell,
  ProviderWorkspaceShell,
  SortableProviderRows,
} from "../components/ProviderWorkspaceShell";
import { Tooltip } from "../components/Tooltip";

interface Props {
  active: boolean;
  records: CodexProviderRecord[];
  /** The stored Codex official-login record, or null when the client has none.
   * Official login is not a third-party profile, so it never appears in
   * `records`; it is one more row the user may create from this page. */
  officialRecord: ProviderRecord | null;
  activeProfileId: string | null;
  busy: boolean;
  onBusy: (busy: boolean) => void;
  onError: (error: CommandError) => void;
  onRefresh: () => Promise<void>;
  onSelectApp: (app: AppKind) => void;
  requestedPreviewId: string | null;
  onPreviewRequestHandled: () => void;
  onImport: () => void;
  onOpenClientSettings: () => void;
  onOpenHistory: () => void;
  /** Routes the destructive delete through the shared confirmation sheet. */
  onDelete: (record: CodexProviderRecord) => void;
  /** Official-login removal goes through the same sheet as a Claude profile. */
  onDeleteOfficial: (record: ProviderRecord) => void;
  /** Codex-only editor session; the shared session state keeps drafts alive
   * across pages exactly like the Claude editor. */
  editorSession: { source: CodexEditorSource | null } | null;
  onNew: () => void;
  onEdit: (record: CodexProviderRecord) => void;
  /** Opens the official-login editor arm; null record creates the entry. */
  onEditOfficial: (record: ProviderRecord | null) => void;
  onCloseEditor: () => void;
  onSave: (draft: CodexProviderDraft) => void;
  onSaveOfficial: (draft: ProviderDraft) => void;
  onSwitchAccessMode: (official: boolean, existing: ProviderRecord | null) => void;
  onSwitchClient: (app: AppKind) => void;
  /** Persists the official record's subscription-quota refresh interval. */
  onSaveOfficialQuotaInterval: (profile: ProviderProfile, minutes: number) => Promise<boolean>;
  statuses: ConfigFileStatus[] | null;
  profiles: ProviderProfile[];
  locks: Partial<Record<AppKind, LockStatus>>;
  userConfigModel: string | null;
  userConfigWarnings: string[];
}

function hostLabel(url: string): string {
  try {
    return new URL(url).host;
  } catch {
    return url;
  }
}

/** The fixed official-login row. It is not sortable: it is the client's
 * native route rather than a member of the ordered third-party list. */
function CodexOfficialRow({ record, active, selected, quotaOpen, previewOpen, onSelect,
  onPreview, onTogglePreview, onEdit, onDelete, onSaveQuotaInterval, onToggleQuota, children }: {
  record: ProviderRecord;
  active: boolean;
  selected: boolean;
  quotaOpen: boolean;
  previewOpen: boolean;
  onSelect: () => void;
  onPreview: () => void;
  onTogglePreview: () => void;
  onEdit: () => void;
  onDelete: () => void;
  onSaveQuotaInterval: (profile: ProviderProfile, minutes: number) => Promise<boolean>;
  onToggleQuota: () => void;
  children?: React.ReactNode;
}) {
  const profile = record.profile;
  const [reloginOpen, setReloginOpen] = useState(false);
  const [quotaNonce, setQuotaNonce] = useState(0);
  const quotaLabel = quotaOpen
    ? `收起 ${profile.name} 订阅额度`
    : `查看 ${profile.name} 订阅额度`;
  return (
    <ProviderRowShell
      id={profile.id}
      name={profile.name}
      active={active}
      selected={selected}
      previewOpen={previewOpen}
      sortable={false}
      onSelect={onSelect}
      meta={<span>官方登录</span>}
      primaryAction={!active ? (
        <Tooltip label={`启用 ${profile.name}`}>
          <Button variant="primary" className="asb-row-activate"
            aria-label={`启用 ${profile.name}`} onClick={onPreview}>
            <PlayIcon size={15} />
            启用
          </Button>
        </Tooltip>
      ) : undefined}
      secondaryAction={(
        <Tooltip label={reloginOpen ? `收起 ${profile.name} 登录` : `重新登录 ${profile.name}`}>
          <Button variant="secondary"
            className={`asb-row-activate${reloginOpen ? " is-active" : ""}`}
            aria-label={reloginOpen ? `收起 ${profile.name} 登录` : `重新登录 ${profile.name}`}
            aria-expanded={reloginOpen}
            onClick={() => setReloginOpen((open) => !open)}>
            {reloginOpen ? "收起登录" : "重新登录"}
          </Button>
        </Tooltip>
      )}
      actions={(
        <>
          <Tooltip label={`编辑 ${profile.name}`}>
            <Button variant="icon" aria-label={`编辑 ${profile.name}`} onClick={onEdit}>
              <EditIcon />
            </Button>
          </Tooltip>
          <Tooltip label={previewOpen ? `收起 ${profile.name} 预览` : `预览 ${profile.name} 变更`}>
            <Button variant="icon" className={previewOpen ? "is-active" : undefined}
              aria-label={previewOpen ? `收起 ${profile.name} 预览` : `预览 ${profile.name} 变更`}
              aria-expanded={previewOpen} onClick={onTogglePreview}>
              {previewOpen ? <EyeOffIcon /> : <PreviewIcon />}
            </Button>
          </Tooltip>
          <Tooltip label={quotaLabel}>
            <Button variant="icon" className={quotaOpen ? "is-active" : undefined}
              aria-label={quotaLabel} aria-controls={`codex-official-quota-${profile.id}`}
              aria-expanded={quotaOpen} onClick={onToggleQuota}>
              <UsageIcon />
            </Button>
          </Tooltip>
          <ProviderMoreActions name={profile.name} onDelete={onDelete} />
        </>
      )}
    >
      {reloginOpen && (
        <OfficialLoginPanel app="codex"
          onFinished={(completed) => { if (completed) setQuotaNonce((nonce) => nonce + 1); }} />
      )}
      {quotaOpen && (
        <CodexOfficialQuotaPanel
          key={`codex-official-quota-${profile.id}-${quotaNonce}`}
          id={`codex-official-quota-${profile.id}`}
          profileId={profile.id}
          profileName={profile.name}
          refreshIntervalMinutes={profile.officialQuotaRefreshIntervalMinutes ?? 0}
          onSaveInterval={(minutes) => onSaveQuotaInterval(profile, minutes)}
        />
      )}
      {children}
    </ProviderRowShell>
  );
}

export function CodexProvidersPage(props: Props) {
  const [preview, setPreview] = useState<{ id: string; file: FilePreview } | null>(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [quotaOpen, setQuotaOpen] = useState(false);
  const previewRevision = useRef(0);

  const run = useCallback(
    async (action: () => Promise<void>) => {
      if (props.busy) return;
      props.onBusy(true);
      try {
        await action();
      } catch (caught) {
        props.onError(caught as CommandError);
      } finally {
        props.onBusy(false);
      }
    },
    [props.busy, props.onBusy, props.onError],
  );

  const dismissPreview = useCallback(() => {
    previewRevision.current += 1;
    setPreview(null);
  }, []);

  const openPreview = useCallback(
    (id: string) =>
      void run(async () => {
        const revision = ++previewRevision.current;
        const file = await previewSwitch(id);
        if (revision === previewRevision.current) {
          setPreview({ id, file });
        }
      }),
    [run],
  );
  useEffect(() => {
    if (!props.active || props.busy || !props.requestedPreviewId) return;
    props.onPreviewRequestHandled();
    openPreview(props.requestedPreviewId);
  }, [
    openPreview,
    props.active,
    props.busy,
    props.onPreviewRequestHandled,
    props.requestedPreviewId,
  ]);
  const apply = () =>
    preview &&
    void run(async () => {
      await executeSwitch(
        preview.id,
        preview.file.contentHash,
        preview.file.renderedHash,
        true,
      );
      dismissPreview();
      await props.onRefresh();
    });

  const reorder = (orderedIds: string[]) =>
    void run(async () => {
      await reorderCodexProfiles(
        orderedIds,
        Object.fromEntries(
          props.records.map((record) => [record.profile.id, record.fileHash]),
        ),
      );
      await props.onRefresh();
    });

  if (props.editorSession)
    return (
      <div hidden={!props.active}>
        <CodexProviderEditor
          active={props.active}
          source={props.editorSession.source}
          busy={props.busy}
          userConfigModel={props.userConfigModel}
          userConfigWarnings={props.userConfigWarnings}
          onSave={props.onSave}
          onSaveOfficial={props.onSaveOfficial}
          onSwitchAccessMode={props.onSwitchAccessMode}
          onCancel={props.onCloseEditor}
          onSwitchClient={props.onSwitchClient}
        />
      </div>
    );

  if (!props.active) return null;

  const ids = props.records.map((record) => record.profile.id);
  const official = props.officialRecord;
  const previewSection = preview && (
    <section className="asb-preview-inline" aria-label="变更预览">
      <div className="asb-panel-heading">
        <h3 className="asb-section-title">变更预览</h3>
        <div className="asb-panel-actions">
          <Button variant="secondary" disabled={props.busy} onClick={dismissPreview}>
            取消
          </Button>
          <Button variant="primary" disabled={props.busy} onClick={apply}>
            确认切换
          </Button>
        </div>
      </div>
      <PreviewInspector
        filePreview={preview.file}
        userConfigModel={props.userConfigModel}
        userConfigWarnings={props.userConfigWarnings}
      />
    </section>
  );

  const officialRow = official && (
    <CodexOfficialRow
      record={official}
      active={props.activeProfileId === official.profile.id}
      selected={selectedId === official.profile.id}
      quotaOpen={quotaOpen}
      previewOpen={preview?.id === official.profile.id}
      onSelect={() => setSelectedId(official.profile.id)}
      onPreview={() => openPreview(official.profile.id)}
      onTogglePreview={() => preview?.id === official.profile.id
        ? dismissPreview()
        : openPreview(official.profile.id)}
      onEdit={() => {
        dismissPreview();
        props.onEditOfficial(official);
      }}
      onDelete={() => props.onDeleteOfficial(official)}
      onSaveQuotaInterval={props.onSaveOfficialQuotaInterval}
      onToggleQuota={() => setQuotaOpen((open) => !open)}
    >
      {preview?.id === official.profile.id && previewSection}
    </CodexOfficialRow>
  );

  return (
    <ProviderWorkspaceShell
      ariaLabel="Codex 供应商"
      app="codex"
      onSelectApp={props.onSelectApp}
      busy={props.busy}
      statuses={props.statuses}
      profiles={props.profiles}
      locks={props.locks}
      onOpenClientSettings={props.onOpenClientSettings}
      onOpenHistory={props.onOpenHistory}
      onImport={props.onImport}
      onNew={() => {
        dismissPreview();
        props.onNew();
      }}
    >
      <SortableProviderRows
        ids={ids}
        onReorder={reorder}
        ariaLabel="Codex 供应商列表"
        emptyLabel="尚无第三方供应商；官方登录已就绪"
        leading={officialRow}
      >
        {props.records.map((record) => {
          const id = record.profile.id;
          const name = record.profile.name;
          const active = props.activeProfileId === id;
          const previewOpen = preview?.id === id;
          const model = record.profile.defaultModel;
          const websiteUrl = record.websiteUrl;
          const host = websiteUrl ? hostLabel(websiteUrl) : null;
          const showsMeta = Boolean(model || host);
          return (
            <ProviderRowShell
              key={id}
              id={id}
              name={name}
              active={active}
              selected={selectedId === id}
              previewOpen={previewOpen}
              sortable
              onSelect={() => setSelectedId(id)}
              meta={showsMeta ? (
                <>
                  {model}
                  {model && host ? " · " : ""}
                  {host && websiteUrl ? (
                    <a
                      className="asb-row-host"
                      href={websiteUrl}
                      title={websiteUrl}
                      onClick={(event) => {
                        event.preventDefault();
                        void openUrl(websiteUrl);
                      }}
                    >
                      {host}
                    </a>
                  ) : null}
                </>
              ) : undefined}
              primaryAction={!active ? (
                <Tooltip label={`启用 ${name}`}>
                  <Button
                    variant="primary"
                    className="asb-row-activate"
                    aria-label={`启用 ${name}`}
                    onClick={() => openPreview(id)}
                  >
                    <PlayIcon size={15} />
                    启用
                  </Button>
                </Tooltip>
              ) : undefined}
              actions={
                <>
                  <Tooltip label={`编辑 ${name}`}>
                    <Button
                      variant="icon"
                      aria-label={`编辑 ${name}`}
                      onClick={() => {
                        dismissPreview();
                        props.onEdit(record);
                      }}
                    >
                      <EditIcon />
                    </Button>
                  </Tooltip>
                  <Tooltip label={previewOpen ? `收起 ${name} 预览` : `预览 ${name} 变更`}>
                    <Button
                      variant="icon"
                      className={previewOpen ? "is-active" : undefined}
                      aria-label={previewOpen ? `收起 ${name} 预览` : `预览 ${name} 变更`}
                      aria-expanded={previewOpen}
                      onClick={() => (previewOpen ? dismissPreview() : openPreview(id))}
                    >
                      {previewOpen ? <EyeOffIcon /> : <PreviewIcon />}
                    </Button>
                  </Tooltip>
                  {!active && (
                    <ProviderMoreActions name={name} onDelete={() => props.onDelete(record)} />
                  )}
                </>
              }
            >
              {previewOpen && previewSection}
            </ProviderRowShell>
          );
        })}
      </SortableProviderRows>
    </ProviderWorkspaceShell>
  );
}
