import type { ReactNode } from "react";
import { Button as MenuButton, Menu, MenuItem, MenuTrigger, Popover } from "react-aria-components";
import type { AppKind } from "../../api/client";
import { EXTENSION_SECTIONS } from "../../app/navigation";
import { clientDeployState } from "../../app/extensions/deployment-state";
import { clientName } from "../../lib/client-name";
import { Button } from "../../components/Button";
import { ClientLogo } from "../../components/ClientLogo";
import { Input } from "../../components/Input";
import { Tabs } from "../../components/Tabs";
import { WorkspaceHeader } from "../../components/WorkspaceHeader";
import { CloseIcon, PlusIcon, SearchIcon, UpdateIcon } from "../../components/icons";
import { Download, History, RefreshCw } from "lucide-react";
import { Tooltip } from "../../components/Tooltip";
import { MANAGEMENT_CLIENTS } from "../../components/extensions/client-presentation";
import { pendingDeployment } from "./pending-deployment";
import type { ExtensionWorkspace } from "./useExtensionWorkspace";

function extensionTabs(workspace: ExtensionWorkspace) {
  return EXTENSION_SECTIONS.map((tab) => ({
    ...tab,
    label: (
      <>
        <span>{tab.label}</span>
        <span className="asb-ext-tab-count asb-num">
          {workspace.items.filter((item) => item.kind === tab.value).length}
        </span>
      </>
    ),
    controls: `ext-workspace-${tab.value}-panel`,
  }));
}

/** The library filter row optionally carries the deployment summaries,
 * keeping search and the counts that qualify it in one operational control. */
export function ExtensionSearch({ kind, search, onSearch, summary }: {
  kind: "skill" | "mcp";
  search: string;
  onSearch: (value: string) => void;
  summary?: ReactNode;
}) {
  return (
    <div className="asb-ext-toolbar" role="search">
      <div className="asb-ext-search">
        <span className="asb-ext-search-icon" aria-hidden="true">
          <SearchIcon />
        </span>
        <Input
          type="search"
          placeholder={kind === "skill" ? "搜索 Skills 名称或描述" : "搜索 MCP 名称、命令或传输方式"}
          aria-label="搜索扩展"
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
            aria-label="清除搜索"
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

export function ExtensionClientSummary({ workspace }: { workspace: ExtensionWorkspace }) {
  const { kindItems, writeBlocked } = workspace;
  return (
    <div className="asb-ext-client-summary" role="group" aria-label="客户端部署数量">
      {MANAGEMENT_CLIENTS.map((client: AppKind) => {
        const state = clientDeployState(kindItems, client);
        const action = `${state.all ? "停用" : "启用"}全部扩展的 ${clientName(client)} 部署`;
        return (
          <Tooltip
            key={client}
            label={state.applicable === 0 ? "没有支持此客户端的扩展" : `${action}，包含当前搜索结果以外的条目`}
          >
            <Button
              variant="unstyled"
              role="checkbox"
              className="asb-ext-client-summary-item"
              data-client={client}
              data-state={state.all ? "all" : state.partial ? "partial" : "none"}
              aria-checked={state.partial ? "mixed" : state.all}
              aria-busy={kindItems.some((item) => pendingDeployment(workspace.applies.pendingOperations, item, client))}
              aria-label={`${action}（当前 ${state.enabled} 项）`}
              disabled={writeBlocked || state.applicable === 0}
              onClick={() => void workspace.toggleAll(client)}
            >
              <ClientLogo app={client} className="asb-ext-client-summary-logo" />
              <span>{clientName(client)}</span>
              <span className="asb-ext-client-summary-value asb-num">{state.enabled}</span>
            </Button>
          </Tooltip>
        );
      })}
    </div>
  );
}

function ExtensionMoreMenu({ workspace }: { workspace: ExtensionWorkspace }) {
  const { nav, writeBlocked } = workspace;
  return (
    <MenuTrigger>
      <MenuButton className="asb-btn asb-btn-secondary" aria-label="更多扩展操作">
        更多
      </MenuButton>
      <Popover placement="bottom end" className="asb-ext-menu">
        <Menu aria-label="更多扩展操作" className="asb-ext-menu-items">
          {nav.kind === "skill" && (
            <MenuItem
              id="new"
              className="asb-ext-menu-item"
              isDisabled={writeBlocked}
              onAction={() => nav.setDialog({ type: "newSkill" })}
            >
              新建本地 Skill
            </MenuItem>
          )}
          <MenuItem
            id="portable"
            className="asb-ext-menu-item"
            isDisabled={writeBlocked}
            onAction={() => nav.setDialog({ type: "portableImport" })}
          >
            导入便携包
          </MenuItem>
          <MenuItem
            id="project"
            className="asb-ext-menu-item"
            isDisabled={writeBlocked}
            onAction={() => nav.setDialog({ type: "project" })}
          >
            注册项目目录
          </MenuItem>
        </Menu>
      </Popover>
    </MenuTrigger>
  );
}

function ExtensionResourceActions({ workspace }: { workspace: ExtensionWorkspace }) {
  const { nav, writeBlocked, kindItems, updates } = workspace;
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
          全部更新（{updates.updatable.length}）
        </Button>
      )}
      {nav.kind === "skill" && (
        <Tooltip label="检查更新">
          <Button variant="icon" aria-label="检查更新"
            disabled={writeBlocked || kindItems.length === 0}
            onClick={() => void updates.check(kindItems.map((item) => item.id))}>
            <RefreshCw />
          </Button>
        </Tooltip>
      )}
      <Tooltip label="操作历史">
        <Button variant="icon" aria-label="操作历史" disabled={workspace.busy}
          onClick={() => nav.setDialog({ type: "history" })}>
          <History />
        </Button>
      </Tooltip>
      <Button
        variant="secondary"
        disabled={workspace.busy}
        onClick={() => nav.setDiscoveryOpen(true)}
      >
        <Download />
        从本机发现
      </Button>
      <Button
        variant="primary"
        disabled={writeBlocked}
        onClick={() =>
          nav.kind === "skill" ? nav.setSourceBrowser(true) : nav.setDialog({ type: "newMcp" })
        }
      >
        {nav.kind === "skill" ? <SearchIcon /> : <PlusIcon />}
        {nav.kind === "skill" ? "发现 Skills" : "添加 MCP"}
      </Button>
      <ExtensionMoreMenu workspace={workspace} />
    </>
  );
}

export function ExtensionToolbar({ workspace }: { workspace: ExtensionWorkspace }) {
  const { nav } = workspace;
  return (
    <WorkspaceHeader
      title="扩展"
      primary={
        <Tabs value={nav.section} onChange={nav.changeSection}
          tabs={extensionTabs(workspace)} scope="ext-workspace" label="扩展内容" />
      }
      primaryActions={nav.kind !== null ? <ExtensionResourceActions workspace={workspace} /> : undefined}
    />
  );
}
