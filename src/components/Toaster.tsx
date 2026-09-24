import { useEffect } from "react";
import { isToastMessage, type Toast, type ToastContent } from "./use-toast";
import { useI18n } from "../i18n";
import { CloseIcon } from "./icons";
import { Button } from "./Button";
import { ToastStatusIcon } from "./ToastStatusIcon";
import { useToast } from "./use-toast";

/**
 * Global floating notifications (Frosted Relay structure replica, user
 * directive 2026-08-31): same stacking limit, error-first ordering, kind
 * based auto-dismiss and hover/focus/hidden pause contract; visuals use this
 * system's tokens (DESIGN.md §8 全局通知).
 */
export function Toaster() {
  const { toasts, dismiss, pause, resume } = useToast();

  useEffect(() => {
    const handleVisibilityChange = () => {
      for (const toast of toasts) {
        if (document.hidden) pause(toast.id);
        else resume(toast.id);
      }
    };
    document.addEventListener("visibilitychange", handleVisibilityChange);
    return () => document.removeEventListener("visibilitychange", handleVisibilityChange);
  }, [pause, resume, toasts]);

  if (toasts.length === 0) return null;

  return (
    <div className="asb-toast-stack">
      {toasts.map((toast) => (
        <ToastItem
          key={toast.id}
          toast={toast}
          onDismiss={dismiss}
          onPause={pause}
          onResume={resume}
        />
      ))}
    </div>
  );
}

interface ToastItemProps {
  toast: Toast;
  onDismiss: (toastId: string) => void;
  onPause: (toastId: string) => void;
  onResume: (toastId: string) => void;
}

/** Renders toast copy in the current language. Descriptors translate on every
 * render so a visible toast follows a language switch; ready-made node
 * content (pre-composed JSX) renders verbatim. Exported for toast-adjacent
 * helpers that compose render-time-translated titles. */
export function ToastText({ content }: { content: ToastContent }) {
  const { t } = useI18n();
  if (isToastMessage(content)) return <>{t(content.key, content.params)}</>;
  return <>{content}</>;
}

function ToastItem({ toast, onDismiss, onPause, onResume }: ToastItemProps) {
  const { id, title, description, kind } = toast;
  const { t } = useI18n();
  return (
    <div
      className="asb-toast"
      onPointerEnter={() => onPause(id)}
      onPointerLeave={() => onResume(id)}
      onFocus={() => onPause(id)}
      onBlur={() => onResume(id)}
    >
      <ToastStatusIcon kind={kind} />
      <div role={kind === "error" ? "alert" : "status"} aria-atomic="true" className="asb-toast-body">
        {title ? <div className="asb-toast-title"><ToastText content={title} /></div> : null}
        {description ? <div className="asb-toast-description"><ToastText content={description} /></div> : null}
      </div>
      <Button
        variant="unstyled"
        className="asb-toast-close"
        aria-label={t("toast.dismiss")}
        onClick={() => onDismiss(id)}
      >
        <CloseIcon />
      </Button>
    </div>
  );
}

/** Retain each message descriptor so visible multi-line notifications translate together. */
export function ToastMessageList({ items }: { items: readonly ToastContent[] }) {
  return <>{items.map((content, index) => <div key={index}><ToastText content={content} /></div>)}</>;
}
