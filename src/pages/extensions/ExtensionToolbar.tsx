import { Button as MenuButton, Menu, MenuItem, MenuTrigger, Popover } from "react-aria-components";
import { EXTENSION_SECTIONS } from "../../app/navigation";
import { Button } from "../../components/Button";
import { Input } from "../../components/Input";
import { Select } from "../../components/Select";
import { Tabs } from "../../components/Tabs";
import { CloseIcon, MoreIcon, PlusIcon, SearchIcon } from "../../components/icons";
import { CLIENT_FILTER_OPTIONS, type ClientFilter } from "./list-filters";
import type { ExtensionWorkspace } from "./useExtensionWorkspace";

const EXTENSION_TOOLBAR_TABS = EXTENSION_SECTIONS.map((tab) => ({
  ...tab,
  controls: `ext-workspace-${tab.value}-panel`,
}));

export function ExtensionFilters({ workspace }: { workspace: ExtensionWorkspace }) {
  const { nav } = workspace;
  if (nav.kind === null) return null;
  return (
    <div className="asb-ext-toolbar">
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
          <button
            type="button"
            className="asb-ext-search-clear"
            aria-label="清除搜索"
            onClick={() => nav.setSearch("")}
          >
            <CloseIcon />
          </button>
        )}
      </div>
      <div className="asb-ext-filter">
        <Select
          value={nav.client}
          options={CLIENT_FILTER_OPTIONS}
          ariaLabel="客户端过滤"
          onChange={(value) => nav.setClient(value as ClientFilter)}
        />
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
