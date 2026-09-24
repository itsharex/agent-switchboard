import type { ReactNode } from "react";
import { Button as MenuButton, Menu, MenuItem, MenuTrigger, Popover } from "react-aria-components";
import type { AppKind } from "../../api/client";
import { EXTENSION_SECTIONS } from "../../app/navigation";
import { clientDeployState } from "../../app/extensions/deployment-state";
import { useI18n, type TFunction } from "../../i18n";
import { clientName } from "../../lib/client-name";
import { Button } from "../../components/Button";
import { ClientLogo } from "../../components/ClientLogo";
import { Input } from "../../components/Input";
import { Tabs } from "../../components/Tabs";
import { WorkspaceHeader } from "../../components/WorkspaceHeader";
import { CloseIcon, PlusIcon, SearchIcon, UpdateIcon } from "../../components/icons";
import { Download } from "lucide-react";
import { Tooltip } from "../../components/Tooltip";
import { MANAGEMENT_CLIENTS } from "../../components/extensions/client-presentation";
import { pendingDeployment } from "./pending-deployment";
import type { ExtensionWorkspace } from "./useExtensionWorkspace";

function extensionTabs(workspace: ExtensionWorkspace, t: TFunction) {
  return EXTENSION_SECTIONS.map((tab) => ({
    ...tab,
    label: (
      <>
        <span>{t(tab.labelKey)}</span>
        <span className="asb-ext-tab-count asb-num">
          {workspace.items.filter((item) => item.kind === tab.value).length}
        </span>
      </>
    ),
    controls: `ext-workspace-${tab.value}-panel`,
  }));
}

/** The library filter row keeps search beside bulk deployment actions,
 * whose full-category scope is labeled separately. */
export function ExtensionSearch({ kind, search, onSearch, summary }: {
  kind: "skill" | "mcp";
  search: string;
  onSearch: (value: string) => void;
  summary?: ReactNode;
}) {
  const { t } = useI18n();
  return (
    <div className="asb-ext-toolbar">
      <div className="asb-ext-search" role="search">
        <span className="asb-ext-search-icon" aria-hidden="true">
          <SearchIcon />
        </span>
        <Input
          type="search"
          placeholder={kind === "skill" ? t("extensions.toolbar.searchSkills") : t("extensions.toolbar.searchMcp")}
          aria-label={t("extensions.toolbar.searchAria")}
          value={search}
          onChange={(event) => onSearch(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Escape" && search) {
              event.stopPropagation();
              onSearch("");
            }
          }}
        />
        {search && (
          <Button
            variant="unstyled"
            className="asb-ext-search-clear"
            aria-label={t("extensions.library.clearSearch")}
            onClick={() => onSearch("")}
          >
            <CloseIcon />
          </Button>
        )}
      </div>
      {summary && <div className="asb-ext-toolbar-summary">{summary}</div>}
    </div>
  );
}

function ExtensionClientSummary({ workspace }: { workspace: ExtensionWorkspace }) {
  const { t } = useI18n();
  const { kindItems, writeBlocked } = workspace;
  const scope = t(workspace.nav.search.trim()
    ? "extensions.toolbar.bulkScopeFiltered"
    : "extensions.toolbar.bulkScope");
  return (
    <div className="asb-ext-client-summary" role="group" aria-label={scope}>
      <span className="asb-ext-client-summary-scope" aria-hidden="true">{scope}</span>
      {MANAGEMENT_CLIENTS.map((client: AppKind) => {
        const state = clientDeployState(kindItems, client);
        const action = t(state.all ? "extensions.toolbar.disableAll" : "extensions.toolbar.enableAll", { client: clientName(client) });
        const shortAction = t(state.all ? "extensions.toolbar.disableClient" : "extensions.toolbar.enableClient", { client: clientName(client) });
        return (
          <Tooltip
            key={client}
            label={state.applicable === 0
              ? t("extensions.toolbar.noneApplicable")
              : action}
          >
            <Button
              variant="unstyled"
              className="asb-ext-client-summary-item"
              data-state={state.all ? "all" : state.partial ? "partial" : "none"}
              aria-busy={kindItems.some((item) => pendingDeployment(workspace.applies.pendingOperations, item, client))}
              aria-label={t("extensions.toolbar.actionAria", { action, count: state.enabled })}
              disabled={writeBlocked || state.applicable === 0}
              onClick={() => void workspace.toggleAll(client)}
            >
              <ClientLogo app={client} className="asb-ext-client-summary-logo" />
              <span>{shortAction}</span>
              <span className="asb-ext-client-summary-value asb-num">
                {t("extensions.toolbar.enabledCount", { count: state.enabled })}
              </span>
            </Button>
          </Tooltip>
        );
      })}
    </div>
  );
}

function ExtensionMoreMenu({ workspace }: { workspace: ExtensionWorkspace }) {
  const { t } = useI18n();
  const { nav, writeBlocked, kindItems, updates, busy } = workspace;
  return (
    <MenuTrigger>
      <MenuButton className="asb-btn asb-btn-secondary" aria-label={t("extensions.toolbar.moreAria")}>
        {t("extensions.toolbar.more")}
      </MenuButton>
      <Popover placement="bottom end" className="asb-ext-menu">
        <Menu aria-label={t("extensions.toolbar.moreAria")} className="asb-ext-menu-items">
          {nav.kind === "skill" && (
            <MenuItem
              id="new"
              className="asb-ext-menu-item"
              isDisabled={writeBlocked}
              onAction={() => nav.setDialog({ type: "newSkill" })}
            >
              {t("extensions.toolbar.newSkill")}
            </MenuItem>
          )}
          {nav.kind === "skill" && (
            <MenuItem
              id="check-updates"
              className="asb-ext-menu-item"
              isDisabled={writeBlocked || kindItems.length === 0}
              onAction={() => void updates.check(kindItems.map((item) => item.id))}
            >
              {t("extensions.management.checkUpdates")}
            </MenuItem>
          )}
          <MenuItem
            id="history"
            className="asb-ext-menu-item"
            isDisabled={busy}
            onAction={() => nav.setDialog({ type: "history" })}
          >
            {t("extensions.toolbar.history")}
          </MenuItem>
          <MenuItem
            id="portable"
            className="asb-ext-menu-item"
            isDisabled={writeBlocked}
            onAction={() => nav.setDialog({ type: "portableImport" })}
          >
            {t("extensions.toolbar.importPortable")}
          </MenuItem>
          <MenuItem
            id="project"
            className="asb-ext-menu-item"
            isDisabled={writeBlocked}
            onAction={() => nav.setDialog({ type: "project" })}
          >
            {t("extensions.toolbar.registerProject")}
          </MenuItem>
        </Menu>
      </Popover>
    </MenuTrigger>
  );
}

/** The action cluster mirrors the client configuration toolbar: uniform
 * bordered commands only; low-frequency upkeep lives in the overflow menu. */
function ExtensionResourceActions({ workspace }: { workspace: ExtensionWorkspace }) {
  const { t } = useI18n();
  const { nav, writeBlocked, updates } = workspace;
  if (nav.kind === null) return null;
  return (
    <>
      {nav.kind === "skill" && updates.updatable.length > 0 && (
        <Button
          variant="secondary"
          disabled={writeBlocked}
          onClick={() => void updates.update(updates.updatable)}
        >
          <UpdateIcon />
          {t("extensions.toolbar.updateAll", { count: updates.updatable.length })}
        </Button>
      )}
      <Button
        variant="secondary"
        disabled={workspace.busy}
        onClick={() => nav.setDiscoveryOpen(true)}
      >
        <Download />
        {t("extensions.discovery.title")}
      </Button>
      <Button
        variant="primary"
        disabled={writeBlocked}
        onClick={() =>
          nav.kind === "skill" ? nav.setSourceBrowser(true) : nav.setDialog({ type: "newMcp" })
        }
      >
        {nav.kind === "skill" ? <SearchIcon /> : <PlusIcon />}
        {nav.kind === "skill" ? t("extensions.sources.title") : t("extensions.toolbar.addMcp")}
      </Button>
      <ExtensionMoreMenu workspace={workspace} />
    </>
  );
}

/** The one header following the workspace-header grammar: row 2 pairs the
 * kind tabs with the page actions, row 3 carries the library search and the
 * full-category client deployment actions. */
export function ExtensionToolbar({ workspace }: { workspace: ExtensionWorkspace }) {
  const { t } = useI18n();
  const { nav } = workspace;
  const controls = nav.kind !== null && workspace.ext.loaded && workspace.ext.workspace ? (
    <ExtensionSearch
      kind={nav.kind}
      search={nav.search}
      onSearch={nav.setSearch}
      summary={<ExtensionClientSummary workspace={workspace} />}
    />
  ) : undefined;
  return (
    <WorkspaceHeader
      title={t("nav.page.extensions")}
      primary={
        <Tabs value={nav.section} onChange={nav.changeSection}
          tabs={extensionTabs(workspace, t)} scope="ext-workspace" label={t("extensions.toolbar.tabsAria")} />
      }
      primaryActions={nav.kind !== null ? <ExtensionResourceActions workspace={workspace} /> : undefined}
      secondary={controls}
    />
  );
}
