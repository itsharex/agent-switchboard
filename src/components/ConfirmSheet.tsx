import { useEffect, useRef, type ReactNode } from "react";

import { Button } from "./Button";

interface Props {
  title: string;
  children: ReactNode;
  confirmLabel: string;
  confirmDisabled?: boolean;
  destructive?: boolean;
  onConfirm: () => void;
  onCancel: () => void;
}

/**
 * Confirmation sheet for irreversible actions. Escape cancels; focus starts
 * on the cancel button so a stray Enter cannot confirm by accident.
 */
export function ConfirmSheet({
  title,
  children,
  confirmLabel,
  confirmDisabled = false,
  destructive = false,
  onConfirm,
  onCancel,
}: Props) {
  const cancelRef = useRef<HTMLButtonElement>(null);
  const sheetRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    cancelRef.current?.focus();
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        onCancel();
        return;
      }
      if (event.key !== "Tab") return;

      const controls = sheetRef.current?.querySelectorAll<HTMLButtonElement>("button:not(:disabled)");
      if (!controls || controls.length === 0) return;
      const first = controls[0];
      const last = controls[controls.length - 1];
      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first.focus();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onCancel]);

  return (
    <div
      className="asb-dialog-backdrop is-inline"
      onClick={(event) => event.target === event.currentTarget && onCancel()}
    >
      <div
        ref={sheetRef}
        className="asb-dialog is-narrow"
        role="dialog"
        aria-modal="true"
        aria-label={title}
      >
        <header className="asb-dialog-heading">
          <h2 className="asb-dialog-title">{title}</h2>
        </header>
        <div className="asb-dialog-body">{children}</div>
        <div className="asb-dialog-footer">
          <Button ref={cancelRef} variant="secondary" onClick={onCancel}>
            取消
          </Button>
          <Button
            variant={destructive ? "danger" : "primary"}
            disabled={confirmDisabled}
            onClick={onConfirm}
          >
            {confirmLabel}
          </Button>
        </div>
      </div>
    </div>
  );
}
