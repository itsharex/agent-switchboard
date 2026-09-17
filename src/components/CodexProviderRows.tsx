import { useState, type ReactNode } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import type { CodexProviderRecord, ProviderProfile, ProviderRecord } from "../api/client";
import { Button } from "./Button";
import { CodexOfficialQuotaPanel } from "./CodexOfficialQuotaPanel";
import { EditIcon, PlayIcon, TrashIcon, UsageIcon } from "./icons";
import { OfficialLoginPanel } from "./OfficialLoginPanel";
import { ProviderRowShell } from "./ProviderWorkspaceShell";
import { Tooltip } from "./Tooltip";

interface RowActionsProps {
  name: string;
  onEdit: () => void;
  onDelete?: () => void;
  children?: ReactNode;
}

function RowActions({ name, onEdit, onDelete, children }: RowActionsProps) {
  return (
    <>
      <Tooltip label={`编辑 ${name}`}>
        <Button variant="icon" aria-label={`编辑 ${name}`} onClick={onEdit}><EditIcon /></Button>
      </Tooltip>
      {children}
      {onDelete && (
        <Tooltip label={`删除 ${name}`}>
          <Button variant="icon" aria-label={`删除 ${name}`} onClick={onDelete}>
            <TrashIcon />
          </Button>
        </Tooltip>
      )}
    </>
  );
}

function ActivateButton({ name, onActivate }: { name: string; onActivate: () => void }) {
  return (
    <Tooltip label={`启用 ${name}`}>
      <Button variant="primary" className="asb-row-activate" aria-label={`启用 ${name}`} onClick={onActivate}>
        <PlayIcon size={15} />
        启用
      </Button>
    </Tooltip>
  );
}

function hostLabel(url: string): string {
  try { return new URL(url).host; } catch { return url; }
}

function ProviderUrl({ url }: { url: string }) {
  return (
    <a className="asb-row-host" href={url} title={url}
      onClick={(event) => { event.preventDefault(); void openUrl(url); }}>
      {hostLabel(url)}
    </a>
  );
}

interface RowProps {
  active: boolean;
  confirmationOpen: boolean;
  onActivate: () => void;
  onEdit: () => void;
  onDelete: () => void;
  children?: ReactNode;
}

export function CodexProviderRow({ record, ...props }: RowProps & { record: CodexProviderRecord }) {
  const { id, name } = record.profile;
  return (
    <ProviderRowShell id={id} name={name} active={props.active}
      confirmationOpen={props.confirmationOpen} sortable
      model={record.profile.defaultModel}
      endpoint={record.websiteUrl ? <ProviderUrl url={record.websiteUrl} /> : <span title={record.profile.endpoint}>{hostLabel(record.profile.endpoint)}</span>}
      primaryAction={!props.active ? <ActivateButton name={name} onActivate={props.onActivate} /> : undefined}
      actions={
        <RowActions name={name} onEdit={props.onEdit}
          onDelete={props.active ? undefined : props.onDelete} />
      }>
      {props.children}
    </ProviderRowShell>
  );
}

interface OfficialRowProps extends RowProps {
  record: ProviderRecord;
  quotaOpen: boolean;
  onSaveQuotaInterval: (profile: ProviderProfile, minutes: number) => Promise<boolean>;
  onToggleQuota: () => void;
  onReloginFinished: () => void;
}

function QuotaToggle({ name, id, open, onToggle }: { name: string; id: string; open: boolean; onToggle: () => void }) {
  const label = open ? `收起 ${name} 订阅额度` : `查看 ${name} 订阅额度`;
  return (
    <Tooltip label={label}>
      <Button variant="icon" className={open ? "is-active" : undefined} aria-label={label}
        aria-controls={`codex-official-quota-${id}`} aria-expanded={open} onClick={onToggle}>
        <UsageIcon />
      </Button>
    </Tooltip>
  );
}

/** Official login participates in the Codex provider order while retaining its own login and quota state. */
export function CodexOfficialRow(props: OfficialRowProps) {
  const { profile } = props.record;
  const [reloginOpen, setReloginOpen] = useState(false);
  const [quotaNonce, setQuotaNonce] = useState(0);
  const loginLabel = reloginOpen ? `收起 ${profile.name} 登录` : `重新登录 ${profile.name}`;
  return (
    <ProviderRowShell id={profile.id} name={profile.name} active={props.active}
      confirmationOpen={props.confirmationOpen} sortable model={profile.model ?? "默认模型"} summary="官方登录"
      primaryAction={!props.active ? <ActivateButton name={profile.name} onActivate={props.onActivate} /> : undefined}
      secondaryAction={
        <Tooltip label={loginLabel}>
          <Button variant="secondary" className={`asb-row-activate${reloginOpen ? " is-active" : ""}`}
            aria-label={loginLabel} aria-expanded={reloginOpen}
            onClick={() => setReloginOpen((open) => !open)}>
            {reloginOpen ? "收起登录" : "重新登录"}
          </Button>
        </Tooltip>
      }
      actions={
        <RowActions name={profile.name} onEdit={props.onEdit}
          onDelete={props.onDelete}>
          <QuotaToggle name={profile.name} id={profile.id} open={props.quotaOpen}
            onToggle={props.onToggleQuota} />
        </RowActions>
      }>
      {reloginOpen && (
        <OfficialLoginPanel app="codex" onFinished={(completed) => {
          if (completed) {
            setQuotaNonce((nonce) => nonce + 1);
            props.onReloginFinished();
          }
        }} />
      )}
      {props.quotaOpen && (
        <CodexOfficialQuotaPanel key={`codex-official-quota-${profile.id}-${quotaNonce}`}
          id={`codex-official-quota-${profile.id}`} profileId={profile.id} profileName={profile.name}
          refreshIntervalMinutes={profile.officialQuotaRefreshIntervalMinutes ?? 0}
          onSaveInterval={(minutes) => props.onSaveQuotaInterval(profile, minutes)} />
      )}
      {props.children}
    </ProviderRowShell>
  );
}
