import { useEffect, useRef, useState } from "react";
import type { Page } from "../app/AppShell";
import { ChevronDownIcon } from "./icons";

interface NavMoreMenuProps {
  /** Low-frequency pages housed behind the trigger, in nav order. */
  items: readonly Page[];
  page: Page;
  onPageChange: (page: Page) => void;
}

/** Overflow disclosure for the primary navigation (2026-09-07 用户指令，方案
 * A)：the trigger reads 「更多」, except while the current page lives in the
 * menu it shows that page's name with the current-page treatment, so the
 * topbar always answers where-am-I. Items inside keep aria-current. */
export function NavMoreMenu({ items, page, onPageChange }: NavMoreMenuProps) {
  const [open, setOpen] = useState(false);
  const root = useRef<HTMLLIElement>(null);
  const trigger = useRef<HTMLButtonElement>(null);

  const active = items.includes(page);

  const close = (refocus = true) => {
    setOpen(false);
    if (refocus) trigger.current?.focus();
  };

  useEffect(() => {
    if (!open) return;
    const onPointerDown = (event: PointerEvent) => {
      if (!root.current?.contains(event.target as Node)) close();
    };
    document.addEventListener("pointerdown", onPointerDown);
    return () => document.removeEventListener("pointerdown", onPointerDown);
  }, [open]);

  return (
    <li
      className="asb-nav-more"
      ref={root}
      onKeyDown={(event) => {
        if (event.key === "Escape" && open) {
          event.preventDefault();
          close();
        }
      }}
    >
      <button
        type="button"
        ref={trigger}
        aria-expanded={open}
        data-active={active || undefined}
        onClick={() => (open ? close(false) : setOpen(true))}
      >
        {active ? page : "更多"}
        <span className="asb-nav-more-chevron" aria-hidden="true">
          <ChevronDownIcon />
        </span>
      </button>
      {open ? (
        <ul className="asb-nav-more-menu" aria-label="更多页面">
          {items.map((item) => (
            <li key={item}>
              <button
                type="button"
                aria-current={page === item ? "page" : undefined}
                onClick={() => {
                  onPageChange(item);
                  close();
                }}
              >
                {item}
              </button>
            </li>
          ))}
        </ul>
      ) : null}
    </li>
  );
}
