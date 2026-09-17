import type { ProviderProfile, ProviderRequestTarget } from "../api/client";
import { openUrl } from "@tauri-apps/plugin-opener";
import { useMemo, useRef, useState } from "react";
import {
  ConnectivityIcon,
  EditIcon,
  TrashIcon,
  UsageIcon,
} from "./icons";
import { CodexOfficialQuotaPanel } from "./CodexOfficialQuotaPanel";
import { Button } from "./Button";
import { OfficialLoginPanel } from "./OfficialLoginPanel";
import { ProviderRowShell, SortableProviderRows } from "./ProviderWorkspaceShell";
import { ProviderUsagePanel } from "./ProviderUsagePanel";
import { useProviderUsage, type ProviderUsage } from "./use-provider-usage";
import { formatUsageHighlight, formatUsageSummary } from "../lib/usage-format";
import { Tooltip } from "./Tooltip";
import { ProviderTestPanel } from "./ProviderTestPanel";

interface Props {
  profiles: ProviderProfile[];
  /** Profile id the live file actually matches, when the app can tell. */
  activeProfileId: string | null;
  /** Model read from the displayed client's user-level configuration file. */
  userConfigModel: string | null;
  /** Persisted profile ids whose usage panel is collapsed; every other
   * configured panel stays expanded. */
  collapsedUsageIds?: string[];
  /** Persists a new display order for the visible client's profiles. */
  onReorder?: (orderedIds: string[]) => void;
  /** Persists the flipped usage-panel state for the profile. */
  onToggleUsage?: (profile: ProviderProfile) => void;
  /** Persists the official Codex quota panel's auto-refresh interval. */
  onSaveQuotaInterval: (profile: ProviderProfile, minutes: number) => Promise<boolean>;
  /** Requests a fresh candidate and the explicit write confirmation. */
  onActivate?: (profile: ProviderProfile) => void;
  onEdit?: (profile: ProviderProfile) => void;
  /** Opens the dedicated usage-query workspace for this profile. */
  onConfigureUsage?: (profile: ProviderProfile) => void;
  onDelete?: (profile: ProviderProfile) => void;
}

function hostLabel(url: string): string {
  try {
    return new URL(url).host;
  } catch {
    return url;
  }
}

interface RowProps {
  profile: ProviderProfile;
  active: boolean;
  userConfigModel: string | null;
  usageOpen: boolean;
  /** Official Codex rows: whether the subscription-quota ledger is unfolded,
   * persisted through the same collapsed-usage owner as `usageOpen`. */
  quotaOpen: boolean;
  sortable: boolean;
  onToggleUsage: (profile: ProviderProfile) => void;
  /** Persists the official Codex quota panel's auto-refresh interval. */
  onSaveQuotaInterval: (profile: ProviderProfile, minutes: number) => Promise<boolean>;
  onActivate?: (profile: ProviderProfile) => void;
  onEdit?: (profile: ProviderProfile) => void;
  onConfigureUsage?: (profile: ProviderProfile) => void;
  onDelete?: (profile: ProviderProfile) => void;
}

function ConfiguredProviderRow(props: RowProps) {
  const usage = useProviderUsage(props.profile);
  return <ProviderRow {...props} usage={usage} />;
}

/** Provider cards use the app's compact routing layout: grip, avatar, name
 * over a detail line, primary action, status pill, and the icon cluster —
 * all owned by the shared row shell. */
function ProviderRow({
  profile,
  active,
  userConfigModel,
  usageOpen,
  quotaOpen,
  sortable,
  onToggleUsage,
  onSaveQuotaInterval,
  onActivate,
  onEdit,
  onConfigureUsage,
  onDelete,
  usage,
}: RowProps & { usage?: ProviderUsage }) {
  const baseUrl = profile.baseUrl;
  const websiteUrl = profile.websiteUrl;
  const [reloginOpen, setReloginOpen] = useState(false);
  /** Bumped on each completed re-login so the quota panel re-queries. */
  const [quotaNonce, setQuotaNonce] = useState(0);
  const [testOpen, setTestOpen] = useState(false);
  const testTriggerRef = useRef<HTMLButtonElement>(null);
  const target = useMemo<ProviderRequestTarget>(() => ({ kind: "saved", profileId: profile.id }),
    [profile.id, profile.baseUrl, profile.apiKey, profile.upstreamProtocol, profile.model, profile.responsesOptions?.requestMode]);
  const official = profile.routeMode === "official";
  const officialQuota = official && profile.app === "codex";
  const displayedModel = active ? userConfigModel : profile.model;
  const modelText = displayedModel ?? "默认模型";
  const modelTitle = active
    ? `从实际用户级配置读取：${modelText}`
    : `档案模型：${modelText}`;
  const hasUsageQuery = profile.usageQuery !== null && profile.usageQuery !== undefined;
  const usageLabel = hasUsageQuery
    ? usageOpen
      ? `收起 ${profile.name} 用量`
      : `查看 ${profile.name} 用量`
    : `配置 ${profile.name} 用量`;
  const quotaLabel = quotaOpen
    ? `收起 ${profile.name} 订阅额度`
    : `查看 ${profile.name} 订阅额度`;
  const testId = `provider-test-${profile.id}`;
  const testLabel = testOpen ? `收起 ${profile.name} 供应商测试` : `测试 ${profile.name} 供应商`;
  const hasClusterActions = Boolean(
    baseUrl ||
      hasUsageQuery ||
      officialQuota ||
      (!official && onConfigureUsage) ||
      onEdit ||
      onDelete,
  );
  const details = !official && !usage ? undefined : (
    <>
      {official && <span>官方登录</span>}
      {usage && (
        <span
          className="asb-row-usage-summary"
          aria-label={`${profile.name} 用量摘要`}
          title={usage.data ? formatUsageSummary(usage.data) : usage.error ?? undefined}
        >
          {usage.data ? formatUsageHighlight(usage.data) : usage.error ? "用量查询失败" : "正在读取用量"}
          {usage.data && usage.error && " · 更新失败"}
          {usage.data && usage.querying && " · 更新中"}
        </span>
      )}
    </>
  );
  const providerUrl = websiteUrl ? (
    <a
      className="asb-row-host"
      href={websiteUrl}
      title={websiteUrl}
      onClick={(event) => {
        // wry blocks webview new-window requests; the opener plugin routes
        // the URL to the system browser instead.
        event.preventDefault();
        void openUrl(websiteUrl);
      }}
    >
      {hostLabel(websiteUrl)}
    </a>
  ) : undefined;

  return (
    <ProviderRowShell
      id={profile.id}
      name={profile.name}
      active={active}
      confirmationOpen={false}
      sortable={sortable}
      model={<span title={modelTitle}>{modelText}</span>}
      endpoint={providerUrl}
      summary={details}
      primaryAction={!active && onActivate ? (
        <Tooltip label={`启用 ${profile.name}`}>
          <Button
            variant="primary"
            className="asb-row-activate"
            aria-label={`启用 ${profile.name}`}
            onClick={() => onActivate(profile)}
          >
            启用
          </Button>
        </Tooltip>
      ) : undefined}
      secondaryAction={official ? (
        <Tooltip label={reloginOpen ? `收起 ${profile.name} 登录` : `重新登录 ${profile.name}`}>
          <Button
            variant="secondary"
            className={`asb-row-activate${reloginOpen ? " is-active" : ""}`}
            aria-label={reloginOpen ? `收起 ${profile.name} 登录` : `重新登录 ${profile.name}`}
            aria-expanded={reloginOpen}
            onClick={() => setReloginOpen((open) => !open)}
          >
            {reloginOpen ? "收起登录" : "重新登录"}
          </Button>
        </Tooltip>
      ) : undefined}
      actions={hasClusterActions ? (
        <>
          {onEdit && (
            <Tooltip label={`编辑 ${profile.name}`}>
              <Button
                variant="icon"
                aria-label={`编辑 ${profile.name}`}
                onClick={() => onEdit(profile)}
              >
                <EditIcon />
              </Button>
            </Tooltip>
          )}
          {!official && baseUrl && (
            <Tooltip label={testLabel}>
              <Button
                ref={testTriggerRef}
                variant="icon"
                className={testOpen ? "is-active" : undefined}
                aria-label={testLabel}
                aria-controls={testId}
                aria-expanded={testOpen}
                onClick={() => setTestOpen((open) => !open)}
              >
                <ConnectivityIcon />
              </Button>
            </Tooltip>
          )}
          {officialQuota && (
            <Tooltip label={quotaLabel}>
              <Button
                variant="icon"
                className={quotaOpen ? "is-active" : undefined}
                aria-label={quotaLabel}
                aria-controls={`codex-official-quota-${profile.id}`}
                aria-expanded={quotaOpen}
                onClick={() => onToggleUsage(profile)}
              >
                <UsageIcon />
              </Button>
            </Tooltip>
          )}
          {!official && (hasUsageQuery || onConfigureUsage) && (
            <Tooltip label={usageLabel}>
              <Button
                variant="icon"
                className={usageOpen ? "is-active" : undefined}
                aria-label={usageLabel}
                aria-controls={hasUsageQuery && usageOpen ? `provider-usage-${profile.id}` : undefined}
                aria-expanded={hasUsageQuery ? usageOpen : undefined}
                onClick={() => {
                  if (hasUsageQuery) onToggleUsage(profile);
                  else onConfigureUsage?.(profile);
                }}
              >
                <UsageIcon />
              </Button>
            </Tooltip>
          )}
          {onDelete && (
            <Tooltip label={`删除 ${profile.name}`}>
              <Button
                variant="icon"
                aria-label={`删除 ${profile.name}`}
                onClick={() => onDelete(profile)}
              >
                <TrashIcon />
              </Button>
            </Tooltip>
          )}
        </>
      ) : undefined}
    >
      {!official && baseUrl && testOpen && (
        <ProviderTestPanel id={testId} name={profile.name} url={baseUrl} target={target}
          onClose={() => { setTestOpen(false); testTriggerRef.current?.focus(); }} />
      )}
      {hasUsageQuery && usageOpen && usage && (
        <ProviderUsagePanel
          id={`provider-usage-${profile.id}`}
          profile={profile}
          usage={usage}
          onConfigure={onConfigureUsage}
        />
      )}
      {reloginOpen && official && (
        <OfficialLoginPanel
          app={profile.app}
          onFinished={(completed) => {
            if (completed) setQuotaNonce((nonce) => nonce + 1);
          }}
        />
      )}
      {officialQuota && quotaOpen && (
        <CodexOfficialQuotaPanel
          key={`codex-official-quota-${profile.id}-${quotaNonce}`}
          id={`codex-official-quota-${profile.id}`}
          profileId={profile.id}
          profileName={profile.name}
          refreshIntervalMinutes={profile.officialQuotaRefreshIntervalMinutes ?? 0}
          onSaveInterval={(minutes) => onSaveQuotaInterval(profile, minutes)}
        />
      )}
    </ProviderRowShell>
  );
}

export function ProviderList({
  profiles,
  activeProfileId,
  userConfigModel,
  collapsedUsageIds = [],
  onReorder,
  onToggleUsage,
  onSaveQuotaInterval,
  onActivate,
  onEdit,
  onConfigureUsage,
  onDelete,
}: Props) {
  const ids = profiles.map((profile) => profile.id);
  return (
    <SortableProviderRows ids={ids} onReorder={onReorder}>
      {profiles.map((profile) => {
        const Row = profile.usageQuery ? ConfiguredProviderRow : ProviderRow;
        return <Row
          key={profile.id}
          profile={profile}
          active={profile.id === activeProfileId}
          userConfigModel={userConfigModel}
          usageOpen={Boolean(profile.usageQuery) && !collapsedUsageIds.includes(profile.id)}
          quotaOpen={
            profile.routeMode === "official" &&
            profile.app === "codex" &&
            !collapsedUsageIds.includes(profile.id)
          }
          sortable={Boolean(onReorder)}
          onToggleUsage={(toggled) => onToggleUsage?.(toggled)}
          onSaveQuotaInterval={onSaveQuotaInterval}
          onActivate={onActivate}
          onEdit={onEdit}
          onConfigureUsage={onConfigureUsage}
          onDelete={onDelete}
        />;
      })}
    </SortableProviderRows>
  );
}
