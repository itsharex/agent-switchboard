import type { AppKind, ExtensionListItem } from "../../api/client";
import {
  clientBindingState,
  itemSupportsClient,
} from "../../app/extensions/deployment-state";
import { useI18n } from "../../i18n";
import { clientName } from "../../lib/client-name";
import { Button } from "../Button";
import { Tooltip } from "../Tooltip";
import { CheckIcon, DashIcon } from "../icons";
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
  const { t } = useI18n();
  return (
    <div className="asb-ext-clienttoggles" role="group" aria-label={t("extensions.clientToggles.aria")}>
      {MANAGEMENT_CLIENTS.map((client) => {
        const supported = itemSupportsClient(item, client);
        const state = clientBindingState(item, client);
        const label = supported
          ? t("extensions.clientToggle.supported", { client: clientName(client), state: clientSummary(item, client) })
          : t("extensions.clientToggle.unsupported", { client: clientName(client) });
        return (
          <Tooltip key={client} label={label} side="bottom">
            <Button
              variant="unstyled"
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
              <span className="asb-ext-clienttoggle-mark" aria-hidden="true">
                {state.all ? <CheckIcon /> : state.partial ? <DashIcon /> : <span className="asb-ext-clienttoggle-empty" />}
              </span>
              <span>{clientName(client)}</span>
            </Button>
          </Tooltip>
        );
      })}
    </div>
  );
}
