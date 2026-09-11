import { Button } from "../../components/Button";
import { PlusIcon, SearchIcon } from "../../components/icons";
import { ExtensionList } from "./ExtensionList";
import { ExtensionFilters } from "./ExtensionToolbar";
import type { ExtensionWorkspace } from "./useExtensionWorkspace";

/** The library's empty state: the one shape from panels.css, with the heading
 * and the action saying which kind of empty this is — nothing added yet, or a
 * filter that matched nothing. */
function EmptyLibrary({ workspace }: { workspace: ExtensionWorkspace }) {
  const empty = workspace.kindItems.length === 0;
  const { nav } = workspace;
  return (
    <div className="asb-empty-state">
      <span className="asb-empty-state-icon" aria-hidden="true">
        {empty ? <PlusIcon /> : <SearchIcon />}
      </span>
      <h3 className="asb-section-title">
        {empty ? `还没有${nav.kind === "skill" ? " Skills" : " MCP 服务"}` : "没有符合过滤条件的扩展"}
      </h3>
      <p className="asb-empty-state-detail">
        {empty ? "添加到扩展库后，选择要启用的客户端。" : "试试其他关键词，或清除筛选条件。"}
      </p>
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
        <p className="asb-empty">正在加载扩展…</p>
      </div>
    );
  if (!workspace.ext.workspace)
    return (
      <div className="asb-empty-state" role="alert">
        <h3 className="asb-section-title">扩展库加载失败</h3>
        <p className="asb-empty-state-detail">重试以读取本机扩展库。</p>
        <Button
          variant="secondary"
          disabled={workspace.busy}
          onClick={() => void workspace.ext.runExclusive(workspace.ext.refresh)}
        >
          重新加载
        </Button>
      </div>
    );
  const filtered = workspace.visible.length !== workspace.kindItems.length;
  return (
    <>
      <ExtensionFilters workspace={workspace} />
      {workspace.visible.length > 0 ? (
        <ExtensionList workspace={workspace} />
      ) : (
        <EmptyLibrary workspace={workspace} />
      )}
      <p className="asb-ext-result-count" role="status">
        <span>
          共 {workspace.kindItems.length} 项
          {filtered && ` · 显示 ${workspace.visible.length} 项`}
        </span>
        <span>
          {" "}
          · 客户端开关作用于整个
          {workspace.nav.kind === "skill" ? " Skills" : " MCP"} 库
        </span>
      </p>
    </>
  );
}
