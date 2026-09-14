import type { AppKind, ExtensionListItem } from "../../api/client";
import {
  clientBindingState,
  itemSupportsClient,
} from "../../app/extensions/deployment-state";
import { clientName } from "../../lib/client-name";
import { ClientLogo } from "../ClientLogo";
import { Button } from "../Button";
import { Tooltip } from "../Tooltip";
import { clientSummary } from "./labels";
import { MANAGEMENT_CLIENTS } from "./client-presentation";

interface Props {
  item: ExtensionListItem;
  busy: boolean;
  pendingClients?: readonly AppKind[];
  /** The typed write pipeline applies intent and handles sensitive confirmation. */
  onToggle: (client: AppKind) => void;
}

/** Mixed means some existing scopes are disabled. The accessible name also
 * carries the file state, independently of the enable/disable intent. */
export function ClientToggleGroup({ item, busy, pendingClients = [], onToggle }: Props) {
  return (
    <div className="asb-ext-clienttoggles" role="group" aria-label="客户端部署开关">
      {MANAGEMENT_CLIENTS.map((client) => {
        const supported = itemSupportsClient(item, client);
        const state = clientBindingState(item, client);
        const label = supported
          ? `${clientName(client)}：${clientSummary(item, client)}`
          : `${clientName(client)}：不支持当前类型`;
        return (
          <Tooltip key={client} label={label} side="bottom">
            <Button
              variant="unstyled"
              data-client={client}
              className="asb-ext-clienttoggle"
              data-state={state.all ? "all" : state.partial ? "partial" : "none"}
              aria-pressed={state.partial ? "mixed" : state.all}
              aria-busy={pendingClients.includes(client)}
              aria-label={label}
              disabled={busy || !supported}
              onClick={(event) => {
                event.stopPropagation();
                onToggle(client);
              }}
            >
              <ClientLogo app={client} className="asb-ext-clienttoggle-logo" />
            </Button>
          </Tooltip>
        );
      })}
    </div>
  );
}
