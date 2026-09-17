import { useCallback, useEffect, useState } from "react";
import {
  executeSwitch, reorderCodexProfiles,
  type AppKind, type CodexProviderDraft, type CodexProviderRecord, type CommandError,
  type ConfigFileStatus, type LockStatus, type ProviderDraft, type ProviderProfile, type ProviderRecord,
} from "../api/client";
import type { CodexEditorSource } from "../app/useProviders";
import { useProviderSwitchFlow } from "../app/useProviderSwitchFlow";
import { Button } from "../components/Button";
import { CodexOfficialRow, CodexProviderRow } from "../components/CodexProviderRows";
import { CodexProviderEditor } from "../components/codex-provider-editor/CodexProviderEditor";
import { PreviewInspector } from "../components/PreviewInspector";
import { ProviderWorkspaceShell, SortableProviderRows } from "../components/ProviderWorkspaceShell";

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
  onImport: () => void;
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
  /** Why third-party switching is blocked right now, or null when the
   * official login is ready; owned by the config snapshot refresh. */
  loginBlocker: string | null;
  statuses: ConfigFileStatus[] | null;
  profiles: ProviderProfile[];
  locks: Partial<Record<AppKind, LockStatus>>;
  userConfigModel: string | null;
  userConfigWarnings: string[];
}

function useCodexProvidersState(props: Props) {
  const [quotaOpen, setQuotaOpen] = useState(false);
  const providerSwitch = useProviderSwitchFlow({ busy: props.busy, onError: props.onError });
  const { activationCandidate, clearCandidates, requestActivation } = providerSwitch;
  const run = useCallback(async (action: () => Promise<void>) => {
    if (props.busy) return;
    props.onBusy(true);
    try { await action(); }
    catch (caught) { props.onError(caught as CommandError); }
    finally { props.onBusy(false); }
  }, [props.busy, props.onBusy, props.onError]);

  useEffect(() => {
    if (!props.active || props.editorSession) clearCandidates();
  }, [props.active, props.editorSession, clearCandidates]);

  const confirmActivation = () => {
    const candidate = activationCandidate;
    if (!candidate) return;
    void run(async () => {
      await executeSwitch(candidate.profileId, candidate.file.contentHash,
        candidate.file.renderedHash, true, candidate.file);
      clearCandidates();
      await props.onRefresh();
    });
  };
  const reorder = (orderedIds: string[], expectedFileHashes: Record<string, string>) => void run(async () => {
    await reorderCodexProfiles(orderedIds, expectedFileHashes);
    await props.onRefresh();
  });
  return {
    activationCandidate,
    clearCandidates,
    quotaOpen,
    setQuotaOpen,
    requestActivation,
    run,
    confirmActivation,
    reorder,
  };
}

type PageState = ReturnType<typeof useCodexProvidersState>;

function CodexProvidersList({ props, state }: { props: Props; state: PageState }) {
  const { activationCandidate } = state;
  const official = props.officialRecord;
  const rows = [
    ...(official ? [{ kind: "official" as const, record: official }] : []),
    ...props.records.map((record) => ({ kind: "provider" as const, record })),
  ].sort((left, right) => left.record.position - right.record.position
    || left.record.profile.id.localeCompare(right.record.profile.id));
  const confirmationSection = activationCandidate && (
    <section className="asb-preview-inline" aria-label="确认切换">
      <div className="asb-panel-heading">
        <h3 className="asb-section-title">确认切换</h3>
        <div className="asb-panel-actions">
          <Button variant="secondary" disabled={props.busy} onClick={state.clearCandidates}>
            取消切换
          </Button>
          <Button variant="primary" disabled={props.busy} onClick={state.confirmActivation}>
            确认切换
          </Button>
        </div>
      </div>
      <PreviewInspector filePreview={activationCandidate.file}
        userConfigModel={props.userConfigModel} userConfigWarnings={props.userConfigWarnings} />
    </section>
  );
  const rowProps = (profile: { id: string }) => ({
    active: props.activeProfileId === profile.id,
    confirmationOpen: activationCandidate?.profileId === profile.id,
    onActivate: () => void state.requestActivation(profile),
  });
  return (
    <>
      <SortableProviderRows
        ids={rows.map((row) => row.record.profile.id)}
        onReorder={(orderedIds) => state.reorder(orderedIds,
          Object.fromEntries(rows.map((row) => [row.record.profile.id, row.record.fileHash])))}
        ariaLabel="Codex 供应商列表"
        emptyLabel="尚无 Codex 供应商"
      >
        {rows.map((row) => row.kind === "official" ? (
          <CodexOfficialRow key={row.record.profile.id} record={row.record} {...rowProps(row.record.profile)} quotaOpen={state.quotaOpen}
            onEdit={() => { state.clearCandidates(); props.onEditOfficial(row.record); }}
            onDelete={() => props.onDeleteOfficial(row.record)}
            onSaveQuotaInterval={props.onSaveOfficialQuotaInterval}
            onToggleQuota={() => state.setQuotaOpen((open) => !open)}
            onReloginFinished={() => void props.onRefresh()}>
            {activationCandidate?.profileId === row.record.profile.id && confirmationSection}
          </CodexOfficialRow>
        ) : (
          <CodexProviderRow key={row.record.profile.id} record={row.record} {...rowProps(row.record.profile)}
            onEdit={() => { state.clearCandidates(); props.onEdit(row.record); }}
            onDelete={() => props.onDelete(row.record)}>
            {activationCandidate?.profileId === row.record.profile.id && confirmationSection}
          </CodexProviderRow>
        ))}
      </SortableProviderRows>
    </>
  );
}

export function CodexProvidersPage(props: Props) {
  const state = useCodexProvidersState(props);
  if (props.editorSession) return (
    <div className="asb-provider-editor-route" hidden={!props.active}>
      <CodexProviderEditor active={props.active} source={props.editorSession.source} busy={props.busy}
        userConfigModel={props.userConfigModel} userConfigWarnings={props.userConfigWarnings}
        onSave={props.onSave} onSaveOfficial={props.onSaveOfficial}
        onSwitchAccessMode={props.onSwitchAccessMode} onCancel={props.onCloseEditor}
        onSwitchClient={props.onSwitchClient} />
    </div>
  );
  if (!props.active) return null;
  return (
      <ProviderWorkspaceShell ariaLabel="Codex 供应商" app="codex" onSelectApp={props.onSelectApp}
      busy={props.busy} statuses={props.statuses} profiles={props.profiles} locks={props.locks}
      onImport={() => { state.clearCandidates(); props.onImport(); }}
      onNew={() => { state.clearCandidates(); props.onNew(); }}>
      <CodexProvidersList props={props} state={state} />
    </ProviderWorkspaceShell>
  );
}
