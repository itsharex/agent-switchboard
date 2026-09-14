import { useState, type ReactNode } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import type { CodexProviderRecord, ProviderProfile, ProviderRecord } from "../api/client";
import { Button } from "./Button";
import { CodexOfficialQuotaPanel } from "./CodexOfficialQuotaPanel";
import { EditIcon, EyeOffIcon, PlayIcon, PreviewIcon, UsageIcon } from "./icons";
import { OfficialLoginPanel } from "./OfficialLoginPanel";
import { ProviderMoreActions } from "./ProviderMoreActions";
import { ProviderRowShell } from "./ProviderWorkspaceShell";
import { Tooltip } from "./Tooltip";

interface RowActionsProps {
  name: string;
  previewOpen: boolean;
  onEdit: () => void;
  onPreview: () => void;
  onDelete?: () => void;
  children?: ReactNode;
}

function RowActions({ name, previewOpen, onEdit, onPreview, onDelete, children }: RowActionsProps) {
  const previewLabel = previewOpen ? `收起 ${name} 预览` : `预览 ${name} 变更`;
  return (
    <>
      <Tooltip label={`编辑 ${name}`}>
        <Button variant="icon" aria-label={`编辑 ${name}`} onClick={onEdit}><EditIcon /></Button>
      </Tooltip>
      <Tooltip label={previewLabel}>
        <Button variant="icon" className={previewOpen ? "is-active" : undefined}
          aria-label={previewLabel} aria-expanded={previewOpen} onClick={onPreview}>
          {previewOpen ? <EyeOffIcon /> : <PreviewIcon />}
        </Button>
      </Tooltip>
      {children}
      {onDelete && <ProviderMoreActions name={name} onDelete={onDelete} />}
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

function ProviderMeta({ record }: { record: CodexProviderRecord }) {
  const model = record.profile.defaultModel;
  const url = record.websiteUrl;
  let host = url;
  if (url) {
    try { host = new URL(url).host; } catch { /* Display the stored value when it is not a URL. */ }
  }
  return (
    <>
      {model}{model && host ? " · " : ""}
      {url && host && (
        <a className="asb-row-host" href={url} title={url}
          onClick={(event) => { event.preventDefault(); void openUrl(url); }}>
          {host}
        </a>
      )}
    </>
  );
}

interface RowProps {
  active: boolean;
  selected: boolean;
  previewOpen: boolean;
  onSelect: () => void;
  onActivate: () => void;
  onTogglePreview: () => void;
  onEdit: () => void;
  onDelete: () => void;
  children?: ReactNode;
}

export function CodexProviderRow({ record, ...props }: RowProps & { record: CodexProviderRecord }) {
  const { id, name } = record.profile;
  const select = (action: () => void) => { props.onSelect(); action(); };
  return (
    <ProviderRowShell legacyLayout onSelect={props.onSelect} id={id} name={name} active={props.active} selected={props.selected}
      previewOpen={props.previewOpen} sortable
      meta={record.profile.defaultModel || record.websiteUrl ? <ProviderMeta record={record} /> : undefined}
      primaryAction={!props.active ? <ActivateButton name={name} onActivate={() => select(props.onActivate)} /> : undefined}
      actions={
        <RowActions name={name} previewOpen={props.previewOpen}
          onEdit={() => select(props.onEdit)} onPreview={() => select(props.onTogglePreview)}
          onDelete={props.active ? undefined : () => select(props.onDelete)} />
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

/** Official login stays a fixed, non-sortable row with its own login and quota state. */
export function CodexOfficialRow(props: OfficialRowProps) {
  const { profile } = props.record;
  const [reloginOpen, setReloginOpen] = useState(false);
  const [quotaNonce, setQuotaNonce] = useState(0);
  const select = (action: () => void) => { props.onSelect(); action(); };
  const loginLabel = reloginOpen ? `收起 ${profile.name} 登录` : `重新登录 ${profile.name}`;
  return (
    <ProviderRowShell legacyLayout onSelect={props.onSelect} id={profile.id} name={profile.name} active={props.active} selected={props.selected}
      previewOpen={props.previewOpen} sortable={false} meta={<span>官方登录</span>}
      primaryAction={!props.active ? <ActivateButton name={profile.name} onActivate={() => select(props.onActivate)} /> : undefined}
      secondaryAction={
        <Tooltip label={loginLabel}>
          <Button variant="secondary" className={`asb-row-activate${reloginOpen ? " is-active" : ""}`}
            aria-label={loginLabel} aria-expanded={reloginOpen}
            onClick={() => select(() => setReloginOpen((open) => !open))}>
            {reloginOpen ? "收起登录" : "重新登录"}
          </Button>
        </Tooltip>
      }
      actions={
        <RowActions name={profile.name} previewOpen={props.previewOpen}
          onEdit={() => select(props.onEdit)} onPreview={() => select(props.onTogglePreview)}
          onDelete={() => select(props.onDelete)}>
          <QuotaToggle name={profile.name} id={profile.id} open={props.quotaOpen}
            onToggle={() => select(props.onToggleQuota)} />
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
