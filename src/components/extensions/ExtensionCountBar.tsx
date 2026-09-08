import type { AppKind, ExtensionListItem } from "../../api/client";
import { clientDeployState, EXTENSION_CLIENTS } from "../../app/extensions/deployment-state";
import { clientName } from "../../lib/client-name";
import { ClientLogo } from "../ClientLogo";
import { Tooltip } from "../Tooltip";

interface Props {
  items: ExtensionListItem[];
  busy: boolean;
  onToggleClient: (client: AppKind) => void;
}

export function ExtensionCountBar({ items, busy, onToggleClient }: Props) {
  return (
    <div className="asb-ext-countbar" aria-label="扩展库客户端启用数量">
      <span className="asb-ext-count-total">共 {items.length} 项</span>
      <div className="asb-ext-count-chips">
        {EXTENSION_CLIENTS.map((client) => {
          const state = clientDeployState(items, client);
          const action = `${state.all ? "停用" : "启用"}全部扩展的 ${clientName(client)} 部署`;
          return (
            <Tooltip
              key={client}
              label={
                state.applicable === 0 ? "没有支持此客户端的扩展" : `${action}，包含当前搜索结果以外的条目`
              }
            >
              <button
                type="button"
                role="checkbox"
                data-client={client}
                data-state={state.all ? "all" : state.partial ? "partial" : "none"}
                aria-checked={state.partial ? "mixed" : state.all}
                aria-label={`${action}（当前 ${state.enabled} 项）`}
                disabled={busy || state.applicable === 0}
                onClick={() => onToggleClient(client)}
                className="asb-ext-count-chip"
              >
                <ClientLogo app={client} className="asb-ext-count-logo" />
                <span>{clientName(client)}</span>
                <span className="asb-ext-count-value">{state.enabled}</span>
              </button>
            </Tooltip>
          );
        })}
      </div>
    </div>
  );
}
