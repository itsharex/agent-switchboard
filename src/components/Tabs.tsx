import { useRef, type ReactNode } from "react";
import { TabButton } from "./TabButton";

interface TabsProps<T extends string> {
  value: T;
  onChange: (value: T) => void;
  scope: string;
  tabs: ReadonlyArray<{ value: T; label: ReactNode; controls?: string; disabled?: boolean }>;
  label: string;
}

/**
 * Segmented tab list. Owns the tab contract: one .asb-tabs pill per tablist,
 * .asb-tab buttons with roving tabindex and arrow / Home / End movement among
 * enabled tabs, and
 * the `${scope}-${value}-tab` button ids that panels reference through
 * aria-labelledby. Consumers wire aria-controls per tab via `controls`.
 */
export function Tabs<T extends string>({ value, onChange, scope, tabs, label }: TabsProps<T>) {
  const buttons = useRef<Array<HTMLButtonElement | null>>([]);
  return (
    <div className="asb-tabs" role="tablist" aria-label={label}>
      {tabs.map((tab, index) => (
        <TabButton
          role="tab"
          selected={value === tab.value}
          tabId={`${scope}-${tab.value}-tab`}
          controls={tab.controls}
          key={tab.value}
          ref={(element) => {
            buttons.current[index] = element;
          }}
          disabled={tab.disabled}
          tabIndex={value === tab.value ? 0 : -1}
          onClick={() => onChange(tab.value)}
          onKeyDown={(event) => {
            if (!["ArrowLeft", "ArrowRight", "Home", "End"].includes(event.key)) return;
            event.preventDefault();
            // Movement stays within enabled tabs; a disabled tab is never
            // selected nor focused (its button is not focusable at all).
            const enabled = tabs
              .map((tab, tabIndex) => (tab.disabled ? -1 : tabIndex))
              .filter((tabIndex) => tabIndex >= 0);
            if (enabled.length === 0) return;
            const forward = event.key === "ArrowRight";
            const position = enabled.indexOf(index);
            const next = event.key === "Home" ? enabled[0]
              : event.key === "End" ? enabled[enabled.length - 1]
              : position === -1
                ? (forward ? enabled[0] : enabled[enabled.length - 1])
                : enabled[(position + (forward ? 1 : enabled.length - 1)) % enabled.length];
            if (next === index) return;
            onChange(tabs[next].value);
            buttons.current[next]?.focus();
          }}
        >
          {tab.label}
        </TabButton>
      ))}
    </div>
  );
}
