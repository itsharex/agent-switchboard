import { useEffect, useRef, useState } from "react";
import { Button } from "./Button";
import { MoreIcon, TrashIcon } from "./icons";
import { Tooltip } from "./Tooltip";

export function ProviderMoreActions({ name, onDelete }: { name: string; onDelete: () => void }) {
  const [open, setOpen] = useState(false);
  const menuRef = useRef<HTMLSpanElement>(null);
  const triggerRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    if (!open) return;
    const onPointerDown = (event: PointerEvent) => {
      if (!menuRef.current?.contains(event.target as Node)) setOpen(false);
    };
    document.addEventListener("pointerdown", onPointerDown);
    return () => document.removeEventListener("pointerdown", onPointerDown);
  }, [open]);

  return (
    <span className="asb-row-more" ref={menuRef} onKeyDown={(event) => {
      if (event.key === "Escape" && open) {
        event.preventDefault();
        setOpen(false);
        triggerRef.current?.focus();
      }
    }}>
      <Tooltip label={`更多 ${name} 操作`}>
        <Button variant="icon" ref={triggerRef} className={open ? "is-active" : undefined}
          aria-label={`更多 ${name} 操作`} aria-haspopup="menu" aria-expanded={open}
          onClick={() => setOpen((value) => !value)}>
          <MoreIcon />
        </Button>
      </Tooltip>
      {open && (
        <span className="asb-row-menu" role="menu" aria-label={`${name} 更多操作`}>
          <Button variant="unstyled" role="menuitem" className="asb-row-menu-item"
            aria-label={`删除 ${name}`} onClick={() => { setOpen(false); onDelete(); }}>
            <TrashIcon size={15} />
            删除
          </Button>
        </span>
      )}
    </span>
  );
}
