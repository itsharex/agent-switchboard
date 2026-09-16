import type { AppKind } from "../../api/client";
import {
  clientDeployState,
} from "../../app/extensions/deployment-state";
import { clientName } from "../../lib/client-name";
import { Button } from "../../components/Button";
import { Tooltip } from "../../components/Tooltip";
import { MANAGEMENT_CLIENTS } from "../../components/extensions/client-presentation";
import { UpdateIcon } from "../../components/icons";
import type { ExtensionWorkspace } from "./useExtensionWorkspace";
import { pendingDeployment } from "./pending-deployment";

/**
 * The library's count bar: the single owner of library-wide deployment
 * counts and update actions.
 *
 * One glass row above the search: the library total on the left, one
 * clickable count chip per client on the right. A chip is the whole
 * library's bulk deploy toggle for that client — click enables or disables
 * every deployment at once through the shared apply pipeline; its state
 * reads none / partial / all. Skills additionally carry the library-wide
 * update-all action at the row's end.
 */
export function ExtensionCountBar({ workspace }: { workspace: ExtensionWorkspace }) {
  const { nav, kindItems, updates, writeBlocked } = workspace;
  if (nav.kind === null) return null;
  const updatable = nav.kind === "skill" ? updates.updatable : [];
  return (
    <div className="asb-ext-count-bar">
      <span className="asb-ext-count-total">
        {nav.kind === "skill" ? "Skills" : "MCP"} · {kindItems.length}
      </span>
      <div className="asb-ext-count-chips" role="group" aria-label="扩展库客户端启用数量">
        {MANAGEMENT_CLIENTS.map((client: AppKind) => {
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
                aria-busy={kindItems.some((item) => pendingDeployment(workspace.applies.pendingOperations, item, client))}
                aria-label={`${action}（当前 ${state.enabled} 项）`}
                disabled={writeBlocked || state.applicable === 0}
                onClick={() => void workspace.toggleAll(client)}
                className="asb-ext-count-chip"
              >
                <span>{clientName(client)}:</span>
                <span className="asb-ext-count-value asb-num">{state.enabled}</span>
              </Button>
            </Tooltip>
          );
        })}
      </div>
      {updatable.length > 0 && (
        <Button
          variant="secondary"
          className="asb-ext-count-update"
          disabled={writeBlocked}
          onClick={() => void updates.update(updatable)}
        >
          <UpdateIcon />
          全部更新（{updatable.length}）
        </Button>
      )}
    </div>
  );
}
