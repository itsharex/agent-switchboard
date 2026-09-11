import { forwardRef, type ButtonHTMLAttributes } from "react";
import { Button } from "./Button";

interface TabButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  selected: boolean;
  controls?: string;
  tabId?: string;
  role?: "button" | "tab";
}

/** Shared tab trigger. Tabs owns the group navigation; this owns each tab's
 * semantic state and visual hook. */
export const TabButton = forwardRef<HTMLButtonElement, TabButtonProps>(
  function TabButton(
    {
      selected,
      controls,
      tabId,
      role = "button",
      className,
      type = "button",
      "aria-selected": _ariaSelected,
      "aria-pressed": _ariaPressed,
      ...props
    },
    ref,
  ) {
    return (
      <Button
        ref={ref}
        variant="unstyled"
        type={type}
        role={role}
        id={tabId}
        aria-selected={role === "tab" ? selected : undefined}
        aria-controls={role === "tab" ? controls : undefined}
        aria-pressed={role === "button" ? selected : undefined}
        className={["asb-tab", selected && "is-on", className].filter(Boolean).join(" ")}
        {...props}
      />
    );
  },
);
