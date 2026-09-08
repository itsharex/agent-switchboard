import { Button } from "../../components/Button";
import { PlusIcon, SearchIcon, UpdateIcon } from "../../components/icons";
import { ExtensionCountBar } from "../../components/extensions/ExtensionCountBar";
import { ExtensionList } from "./ExtensionList";
import { ExtensionFilters } from "./ExtensionToolbar";
import type { ExtensionWorkspace } from "./useExtensionWorkspace";

function EmptyLibrary({ workspace }: { workspace: ExtensionWorkspace }) {
  const empty = workspace.kindItems.length === 0;
  const { nav } = workspace;
  return (
    <div className="asb-ext-empty">
      <span className="asb-ext-empty-icon" aria-hidden="true">
        {empty ? <PlusIcon /> : <SearchIcon />}
      </span>
      <h3>{empty ? `还没有${nav.kind === "skill" ? " Skills" : " MCP 服务"}` : "没有符合过滤条件的扩展"}</h3>
      <p>{empty ? "添加到扩展库后，选择要启用的客户端。" : "试试其他关键词，或清除筛选条件。"}</p>
      {empty ? (
        <Button
          variant="secondary"
          disabled={workspace.writeBlocked}
          onClick={() =>
            nav.kind === "skill" ? nav.setSourceBrowser(true) : nav.setDialog({ type: "newMcp" })
          }
        >
          {nav.kind === "skill" ? "发现 Skills" : "添加 MCP"}
        </Button>
      ) : (
        <Button variant="secondary" onClick={nav.clearFilters}>
          清除筛选
        </Button>
      )}
    </div>
  );
}

export function ExtensionLibraryPanel({ workspace }: { workspace: ExtensionWorkspace }) {
  if (workspace.nav.kind === null) return null;
  if (!workspace.ext.loaded)
    return (
      <div className="asb-ext-loading" role="status">
        正在加载扩展…
      </div>
    );
  if (!workspace.ext.workspace)
    return (
      <div className="asb-ext-empty" role="alert">
        <h3>扩展库加载失败</h3>
        <p>重试以读取本机扩展库。</p>
        <Button
          variant="secondary"
          disabled={workspace.busy}
          onClick={() => void workspace.ext.runExclusive(workspace.ext.refresh)}
        >
          重新加载
        </Button>
      </div>
    );
  return (
    <>
      <div className="asb-ext-countbar-row">
        <ExtensionCountBar
          items={workspace.kindItems}
          busy={workspace.writeBlocked}
          onToggleClient={(client) => void workspace.toggleAll(client)}
        />
        {workspace.nav.kind === "skill" && workspace.updates.updatable.length > 0 && (
          <Button
            variant="secondary"
            disabled={workspace.writeBlocked}
            onClick={() => void workspace.updates.update(workspace.updates.updatable)}
          >
            <UpdateIcon />
            全部更新（{workspace.updates.updatable.length}）
          </Button>
        )}
      </div>
      <ExtensionFilters workspace={workspace} />
      {workspace.visible.length > 0 ? (
        <ExtensionList workspace={workspace} />
      ) : (
        <EmptyLibrary workspace={workspace} />
      )}
      {workspace.visible.length !== workspace.kindItems.length && (
        <p className="asb-ext-result-count" role="status">
          显示 {workspace.visible.length} / {workspace.kindItems.length} 项 · 上方客户端开关作用于整个
          {workspace.nav.kind === "skill" ? " Skills" : " MCP"} 库
        </p>
      )}
    </>
  );
}
