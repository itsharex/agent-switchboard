import type { ActivationCandidate } from "../app/useProviderSwitchFlow";
import type { ProviderProfile, ProviderRequestTarget } from "../api/client";
import { openUrl } from "@tauri-apps/plugin-opener";
import { useMemo, useRef, useState, type ReactNode } from "react";
import {
  ConnectivityIcon,
  EditIcon,
  TrashIcon,
  UsageIcon,
} from "./icons";
import { Button } from "./Button";
import { OfficialLoginPanel } from "./OfficialLoginPanel";
import { ProviderRowShell, SortableProviderRows } from "./ProviderWorkspaceShell";
import { ProviderUsagePanel } from "./ProviderUsagePanel";
import { RowPanelDisclosure } from "./RowPanelDisclosure";
import { useProviderUsage, type ProviderUsage } from "./use-provider-usage";
import { formatUsageHighlight, formatUsageSummary } from "../lib/usage-format";
import { SwitchConfirmationSection } from "./SwitchConfirmationSection";
import { Tooltip } from "./Tooltip";
import { ProviderTestPanel } from "./ProviderTestPanel";

interface Props {
  profiles: ProviderProfile[];
  /** Profile id the live file actually matches, when the app can tell. */
  activeProfileId: string | null;
  /** Model read from the displayed client's user-level configuration file. */
  userConfigModel: string | null;
  /** Known conditions that can override the user-level configuration. */
  userConfigWarnings: string[];
  busy: boolean;
  /** Persisted profile ids whose usage panel is collapsed; every other
   * configured panel stays expanded. */
  collapsedUsageIds?: string[];
  /** The pending switch candidate; its row unfolds the confirmation. */
  activationCandidate: ActivationCandidate | null;
  /** Persists a new display order for the visible client's profiles. */
  onReorder?: (orderedIds: string[]) => void;
  /** Persists the flipped usage-panel state for the profile. */
  onToggleUsage?: (profile: ProviderProfile) => void;
  /** Requests a fresh candidate and the explicit write confirmation. */
  onActivate?: (profile: ProviderProfile) => void;
  /** Confirms the pending candidate through the switch executor. */
  onConfirmSwitch: () => void;
  /** Discards the pending candidate without writing. */
  onCancelActivation: () => void;
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
  sortable: boolean;
  /** The pending switch confirmation, already matched to this row. */
  confirmation?: ReactNode;
  onToggleUsage: (profile: ProviderProfile) => void;
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
  sortable,
  confirmation,
  onToggleUsage,
  onActivate,
  onEdit,
  onConfigureUsage,
  onDelete,
  usage,
}: RowProps & { usage?: ProviderUsage }) {
  const baseUrl = profile.baseUrl;
  const websiteUrl = profile.websiteUrl;
  const [reloginOpen, setReloginOpen] = useState(false);
  const [testOpen, setTestOpen] = useState(false);
  const testTriggerRef = useRef<HTMLButtonElement>(null);
  const target = useMemo<ProviderRequestTarget>(() => ({ kind: "saved", profileId: profile.id }),
    [profile.id, profile.baseUrl, profile.apiKey, profile.upstreamProtocol, profile.model, profile.responsesOptions?.requestMode]);
  const official = profile.routeMode === "official";
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
  const testId = `provider-test-${profile.id}`;
  const testLabel = testOpen ? `收起 ${profile.name} 供应商测试` : `测试 ${profile.name} 供应商`;
  const hasClusterActions = Boolean(
    baseUrl ||
      hasUsageQuery ||
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
          {usage.data ? formatUsageHighlight(usage.data) : usage.error ? "用量查询失败" : usage.querying ? "正在读取用量" : "暂无用量读数"}
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
      confirmationOpen={confirmation !== undefined}
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
      {!official && baseUrl && (
        <RowPanelDisclosure open={testOpen}>
          <ProviderTestPanel id={testId} name={profile.name} url={baseUrl} target={target}
            onClose={() => { setTestOpen(false); testTriggerRef.current?.focus(); }} />
        </RowPanelDisclosure>
      )}
      {usage && (
        <RowPanelDisclosure open={usageOpen}>
          <ProviderUsagePanel
            id={`provider-usage-${profile.id}`}
            name={profile.name}
            usage={usage}
            onConfigure={onConfigureUsage ? () => onConfigureUsage(profile) : undefined}
          />
        </RowPanelDisclosure>
      )}
      {confirmation}
      {reloginOpen && official && (
        <OfficialLoginPanel app={profile.app} />
      )}
    </ProviderRowShell>
  );
}

/** Claude's list; Codex rows and official quotas belong to CodexProvidersPage. */
export function ProviderList({
  profiles,
  activeProfileId,
  userConfigModel,
  userConfigWarnings,
  busy,
  collapsedUsageIds = [],
  activationCandidate,
  onReorder,
  onToggleUsage,
  onActivate,
  onConfirmSwitch,
  onCancelActivation,
  onEdit,
  onConfigureUsage,
  onDelete,
}: Props) {
  const ids = profiles.map((profile) => profile.id);
  return (
    <SortableProviderRows ids={ids} onReorder={onReorder}>
      {profiles.map((profile) => {
        const Row = profile.usageQuery ? ConfiguredProviderRow : ProviderRow;
        const confirming = activationCandidate?.profileId === profile.id;
        return <Row
          key={profile.id}
          profile={profile}
          active={profile.id === activeProfileId}
          userConfigModel={userConfigModel}
          usageOpen={Boolean(profile.usageQuery) && !collapsedUsageIds.includes(profile.id)}
          sortable={Boolean(onReorder)}
          confirmation={confirming && activationCandidate ? (
            <SwitchConfirmationSection filePreview={activationCandidate.file}
              busy={busy} userConfigModel={userConfigModel}
              userConfigWarnings={userConfigWarnings}
              onConfirm={onConfirmSwitch} onCancel={onCancelActivation} />
          ) : undefined}
          onToggleUsage={(toggled) => onToggleUsage?.(toggled)}
          onActivate={onActivate}
          onEdit={onEdit}
          onConfigureUsage={onConfigureUsage}
          onDelete={onDelete}
        />;
      })}
    </SortableProviderRows>
  );
}
