import { Button } from "../../components/Button";
import { SearchIcon } from "../../components/icons";
import { Server, Sparkles } from "lucide-react";
import { ExtensionCountBar } from "./ExtensionCountBar";
import { ExtensionList } from "./ExtensionList";
import { ExtensionSearch } from "./ExtensionToolbar";
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
        {empty ? nav.kind === "skill" ? <Sparkles /> : <Server /> : <SearchIcon />}
      </span>
      <h3 className="asb-section-title">
        {empty ? `还没有${nav.kind === "skill" ? " Skills" : " MCP 服务"}` : "没有符合过滤条件的扩展"}
      </h3>
      {!empty && (
        <Button variant="secondary" onClick={nav.clearFilters}>
          清除搜索
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
      <ExtensionCountBar workspace={workspace} />
      <ExtensionSearch kind={workspace.nav.kind} search={workspace.nav.search} onSearch={workspace.nav.setSearch} />
      <div className="asb-ext-library-scroll">
        {workspace.visible.length > 0 ? (
          <ExtensionList workspace={workspace} />
        ) : (
          <EmptyLibrary workspace={workspace} />
        )}
        <p className="asb-ext-result-count" role="status">
          共 {workspace.kindItems.length} 项
          {filtered && ` · 显示 ${workspace.visible.length} 项`}
        </p>
      </div>
    </>
  );
}
