import { useId, type ReactNode } from "react";
import { Button } from "../Button";
import { ModuleHeader } from "../WorkspaceHeader";

interface ClientManagementModuleProps {
  title: string;
  description: ReactNode;
  refreshLabel: string;
  busy: boolean;
  refreshDisabled?: boolean;
  onRefresh: () => void;
  children: ReactNode;
}

/** Shared frame for the one client-specific module shown in each client tab. */
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
      <ModuleHeader
        title={title}
        id={titleId}
        primaryActions={<Button variant="secondary" disabled={refreshDisabled ?? busy} onClick={onRefresh}>{refreshLabel}</Button>}
      />
      <p className="asb-scope-note">{description}</p>
      <div className="asb-client-management-module-content">{children}</div>
    </section>
  );
}