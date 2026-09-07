import type { LucideIcon } from "lucide-react";
import { CircleCheck, CircleX, Info, TriangleAlert } from "lucide-react";
import type { ToastKind } from "./use-toast";

const ICONS: Record<ToastKind, { Icon: LucideIcon; tone: string }> = {
  info: { Icon: Info, tone: "asb-toast-icon--info" },
  success: { Icon: CircleCheck, tone: "asb-toast-icon--success" },
  warning: { Icon: TriangleAlert, tone: "asb-toast-icon--warning" },
  error: { Icon: CircleX, tone: "asb-toast-icon--error" },
};

interface ToastStatusIconProps {
  kind: ToastKind;
}

export function ToastStatusIcon({ kind }: ToastStatusIconProps) {
  const { Icon, tone } = ICONS[kind];
  return (
    <Icon
      aria-hidden="true"
      className={`asb-toast-icon ${tone}`}
      size={20}
      strokeWidth={1.8}
    />
  );
}
