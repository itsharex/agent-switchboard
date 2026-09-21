import { useLayoutEffect, useRef, useState, type ReactNode } from "react";

interface Props {
  /** Whether the panel is expanded; the content unmounts after the collapse
   * transition ends, so a closed row keeps the same fixed data rail. */
  open: boolean;
  children: ReactNode;
}

/** The inline disclosure for provider-row panels (供应商测试 / 用量读数).
 * Content mounts when opening and stays mounted through the collapse
 * transition, then unmounts: usage details exist in the DOM only while
 * expanded, nothing runs or polls while hidden, and the collapsed row keeps
 * the fixed primary rail. The expand transition needs one collapsed layout
 * pass, so the effect forces a reflow before applying the open class. */
export function RowPanelDisclosure({ open, children }: Props) {
  const [mounted, setMounted] = useState(open);
  const [expanded, setExpanded] = useState(open);
  const frameRef = useRef<HTMLDivElement | null>(null);
  useLayoutEffect(() => {
    if (open) setMounted(true);
    else setExpanded(false);
  }, [open]);
  useLayoutEffect(() => {
    if (!mounted || !open || expanded) return;
    frameRef.current?.getBoundingClientRect();
    setExpanded(true);
  }, [mounted, open, expanded]);
  if (!mounted) return null;
  return (
    <div
      ref={frameRef}
      className={`asb-row-panel-disclosure${expanded ? " is-open" : ""}`}
      onTransitionEnd={(event) => {
        // Collapsed descendants fire their own transitions into this bubble.
        if (expanded || event.target !== event.currentTarget) return;
        if (event.propertyName === "grid-template-rows") setMounted(false);
      }}
    >
      {children}
    </div>
  );
}
