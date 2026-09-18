import { useId, type ReactNode } from "react";
import { RefreshCw } from "lucide-react";
import { Button } from "../Button";

interface ClientManagementModuleProps {
  title: string;
  description: ReactNode;
  /** Accessible name of the icon-only refresh control. */
  refreshLabel: string;
  busy: boolean;
  refreshDisabled?: boolean;
  onRefresh: () => void;
  children: ReactNode;
}

/** Shared frame for the one client-specific module shown in each client tab.
 * The page header and the client tabs are the only heading pair, so the
 * module identifies itself with a quiet eyebrow and an icon refresh action
 * instead of a second module header. */
export function ClientManagementModule({
  title,
  description,
  refreshLabel,
  busy,
  refreshDisabled,
  onRefresh,
  children,
}: ClientManagementModuleProps) {
  const titleId = useId();
  return (
    <section className="asb-client-management-module" aria-labelledby={titleId} aria-busy={busy || undefined}>
      <div className="asb-client-management-bar">
        <h3 id={titleId} className="asb-client-management-eyebrow">{title}</h3>
        <Button variant="icon" aria-label={refreshLabel} title={refreshLabel}
          disabled={refreshDisabled ?? busy} onClick={onRefresh}><RefreshCw /></Button>
      </div>
      <p className="asb-scope-note">{description}</p>
      <div className="asb-client-management-module-content">{children}</div>
    </section>
  );
}
