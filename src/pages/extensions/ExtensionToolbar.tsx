import { Button as MenuButton, Menu, MenuItem, MenuTrigger, Popover } from "react-aria-components";
import { EXTENSION_SECTIONS } from "../../app/navigation";
import { Button } from "../../components/Button";
import { Input } from "../../components/Input";
import { Tabs } from "../../components/Tabs";
import { WorkspaceHeader } from "../../components/WorkspaceHeader";
import { CloseIcon, MoreIcon, PlusIcon, SearchIcon } from "../../components/icons";
import { Download, History, RefreshCw } from "lucide-react";
import { Tooltip } from "../../components/Tooltip";
import type { ExtensionWorkspace } from "./useExtensionWorkspace";

const EXTENSION_TOOLBAR_TABS = EXTENSION_SECTIONS.map((tab) => ({
  ...tab,
  controls: `ext-workspace-${tab.value}-panel`,
}));

/**
 * The search row, cc-switch's ManagementListSearch counterpart: one search
 * field directly above the list, inside the content column. The library-wide
 * count bar (deploy toggles, update-all) owns the row above it.
 */
export function ExtensionSearch({ kind, search, onSearch }: {
  kind: "skill" | "mcp";
  search: string;
  onSearch: (value: string) => void;
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
    </div>
  );
}

function ExtensionMoreMenu({ workspace }: { workspace: ExtensionWorkspace }) {
  const { nav, writeBlocked } = workspace;
  return (
    <MenuTrigger>
      <MenuButton className="asb-btn asb-btn-icon" aria-label="更多扩展操作">
        <MoreIcon />
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
  const alternateView = nav.sourceBrowser || nav.discoveryOpen;
  if (nav.kind === null || alternateView) return null;
  return (
    <>
      {nav.kind === "skill" && !nav.sourceBrowser && (
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
    </>
  );
}

export function ExtensionToolbar({ workspace }: { workspace: ExtensionWorkspace }) {
  const { nav } = workspace;
  return (
    <WorkspaceHeader
      title="扩展"
      back={nav.sourceBrowser ? (
        <Button variant="back" aria-label="返回扩展库" onClick={() => nav.setSourceBrowser(false)}>←</Button>
      ) : nav.discoveryOpen ? (
        <Button variant="back" aria-label="返回扩展库" onClick={() => nav.setDiscoveryOpen(false)}>←</Button>
      ) : undefined}
      primary={
        <Tabs value={nav.section} onChange={nav.changeSection}
          tabs={EXTENSION_TOOLBAR_TABS} scope="ext-workspace" label="扩展内容" />
      }
      primaryActions={nav.kind !== null ? <ExtensionResourceActions workspace={workspace} /> : undefined}
    />
  );
}
