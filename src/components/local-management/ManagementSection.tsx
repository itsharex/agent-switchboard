import { useState, type ReactNode } from "react";

/** A lazy disclosure keeps a grouped dialog compact and does not load unrelated local data. */
export function ManagementSection({ title, initiallyOpen = false, children }: {
  title: string;
  initiallyOpen?: boolean;
  children: ReactNode;
}) {
  const [open, setOpen] = useState(initiallyOpen);
  return <details className="asb-provider-disclosure asb-management-section" open={open}
    onToggle={(event) => setOpen(event.currentTarget.open)}>
    <summary>{title}</summary>
    {open && <div className="asb-provider-disclosure-body">{children}</div>}
  </details>;
}
