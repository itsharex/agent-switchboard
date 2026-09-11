import { Button as MenuButton, Menu, MenuItem, MenuTrigger, Popover } from "react-aria-components";
import { EXTENSION_SECTIONS } from "../../app/navigation";
import { Button } from "../../components/Button";
import { ClientFilter } from "../../components/ClientFilter";
import { ClientLogo } from "../../components/ClientLogo";
import { Input } from "../../components/Input";
import { Tabs } from "../../components/Tabs";
import { Tooltip } from "../../components/Tooltip";
import { CloseIcon, MoreIcon, PlusIcon, SearchIcon, UpdateIcon } from "../../components/icons";
import { clientDeployState, EXTENSION_CLIENTS } from "../../app/extensions/deployment-state";
import { clientName } from "../../lib/client-name";
import type { ExtensionWorkspace } from "./useExtensionWorkspace";

const EXTENSION_TOOLBAR_TABS = EXTENSION_SECTIONS.map((tab) => ({
  ...tab,
  controls: `ext-workspace-${tab.value}-panel`,
}));

/**
 * The library's single tool bar (2026-09-11 user directive).
 *
 * View controls — search and the client filter — sit left; library-wide
 * actions — the per-client bulk deployment toggles and update-all — sit right.
 * The old layout stacked a count bar, a search row and a result line, which
 * put three different control heights on one screen; the client filter was
 * additionally a tablist here and a radio group on the session page. There is
 * now one bar, one client-filter idiom, and the list total is a caption under
 * the list rather than a row of its own.
 */
export function ExtensionFilters({ workspace }: { workspace: ExtensionWorkspace }) {
  const { nav, updates, kindItems, writeBlocked } = workspace;
  if (nav.kind === null) return null;
  const updatable = nav.kind === "skill" ? updates.updatable : [];
  return (
    <div className="asb-ext-toolbar">
      <div className="asb-ext-toolbar-view">
        <div className="asb-ext-search">
          <span className="asb-ext-search-icon" aria-hidden="true">
            <SearchIcon />
          </span>
          <Input
            type="search"
            placeholder={nav.kind === "skill" ? "搜索 Skills 名称或描述" : "搜索 MCP 名称、命令或传输方式"}
            aria-label="搜索扩展"
            value={nav.search}
            onChange={(event) => nav.setSearch(event.target.value)}
          />
          {nav.search && (
            <Button
              variant="unstyled"
              className="asb-ext-search-clear"
              aria-label="清除搜索"
              onClick={() => nav.setSearch("")}
            >
              <CloseIcon />
            </Button>
          )}
        </div>
        <div className="asb-ext-filter">
          <ClientFilter
            value={nav.client}
            onChange={nav.setClient}
            label="客户端过滤"
            showLogos
          />
        </div>
      </div>
      <div className="asb-ext-toolbar-actions">
        <div
          className="asb-ext-count-chips"
          role="group"
          aria-label="扩展库客户端启用数量"
        >
          {EXTENSION_CLIENTS.map((client) => {
            const state = clientDeployState(kindItems, client);
            const action = `${state.all ? "停用" : "启用"}全部扩展的 ${clientName(client)} 部署`;
            return (
              <Tooltip
                key={client}
                label={
                  state.applicable === 0
                    ? "没有支持此客户端的扩展"
                    : `${action}，包含当前搜索结果以外的条目`
                }
              >
                <Button
                  variant="unstyled"
                  role="checkbox"
                  data-client={client}
                  data-state={state.all ? "all" : state.partial ? "partial" : "none"}
                  aria-checked={state.partial ? "mixed" : state.all}
                  aria-label={`${action}（当前 ${state.enabled} 项）`}
                  disabled={writeBlocked || state.applicable === 0}
                  onClick={() => void workspace.toggleAll(client)}
                  className="asb-ext-count-chip"
                >
                  <ClientLogo app={client} className="asb-ext-count-logo" />
                  <span>{clientName(client)}</span>
                  <span className="asb-ext-count-value">{state.enabled}</span>
                </Button>
              </Tooltip>
            );
          })}
        </div>
        {updatable.length > 0 && (
          <Button
            variant="secondary"
            disabled={writeBlocked}
            onClick={() => void updates.update(updatable)}
          >
            <UpdateIcon />
            全部更新（{updatable.length}）
          </Button>
        )}
      </div>
    </div>
  );
}

function ExtensionMoreMenu({ workspace }: { workspace: ExtensionWorkspace }) {
  const { nav, writeBlocked, kindItems, updates } = workspace;
  return (
    <MenuTrigger>
      <MenuButton className="asb-btn asb-btn-icon" aria-label="更多扩展操作">
        <MoreIcon />
      </MenuButton>
      <Popover placement="bottom end" className="asb-ext-menu">
        <Menu aria-label="更多扩展操作" className="asb-ext-menu-items">
          {nav.kind === "skill" && (
            <MenuItem
              id="check"
              className="asb-ext-menu-item"
              isDisabled={writeBlocked || kindItems.length === 0}
              onAction={() => void updates.check(kindItems.map((item) => item.id))}
            >
              检查更新
            </MenuItem>
          )}
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
          <MenuItem
            id="history"
            className="asb-ext-menu-item"
            onAction={() => nav.setDialog({ type: "history" })}
          >
            操作历史
          </MenuItem>
        </Menu>
      </Popover>
    </MenuTrigger>
  );
}

function ExtensionResourceActions({ workspace }: { workspace: ExtensionWorkspace }) {
  const { nav, writeBlocked } = workspace;
  if (nav.kind === null) return null;
  return (
    <div className="asb-ext-actions">
      <Button
        variant="secondary"
        disabled={workspace.busy}
        onClick={() => nav.setDialog({ type: "import" })}
      >
        从本机发现
      </Button>
      {nav.sourceBrowser ? null : (
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
      )}
      <ExtensionMoreMenu workspace={workspace} />
    </div>
  );
}

export function ExtensionToolbar({ workspace }: { workspace: ExtensionWorkspace }) {
  return (
    <header className="asb-panel-heading">
      <div className="asb-panel-heading-main">
        {workspace.nav.sourceBrowser && (
          <Button variant="back" aria-label="返回扩展库" onClick={() => workspace.nav.setSourceBrowser(false)}>←</Button>
        )}
        <h2 className="asb-panel-title">扩展</h2>
        <Tabs value={workspace.nav.section} onChange={workspace.nav.changeSection}
          tabs={EXTENSION_TOOLBAR_TABS} scope="ext-workspace" label="扩展内容" />
      </div>
      <ExtensionResourceActions workspace={workspace} />
    </header>
  );
}
