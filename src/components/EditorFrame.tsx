import type { ReactNode, Ref } from "react";
import { Button } from "./Button";
import { WorkspaceHeader } from "./WorkspaceHeader";

interface Props {
  title: string;
  titleRef?: Ref<HTMLHeadingElement>;
  /** Destination name, carried as the back control's accessible label. */
  backLabel: string;
  busy?: boolean;
  /** Header row 2 context (tabs, provider name) beside the back control. */
  primary?: ReactNode;
  onBack: () => void;
  /** Surface modifier for caller-specific visuals, scoped below the frame contract. */
  className?: string;
  children: ReactNode;
  /** Fixed bottom action bar; its content is owned by the composing surface.
   * Surfaces without a persistent commit action omit it. */
  footer?: ReactNode;
}

/** Owns the full-page editor frame: a pinned header row with the back
 * control, a scrollable module body, and a pinned bottom action bar. */
export function EditorFrame({
  title,
  titleRef,
  backLabel,
  busy = false,
  primary,
  onBack,
  className,
  children,
  footer,
}: Props) {
  return (
    <div className={className ? `asb-editor-frame ${className}` : "asb-editor-frame"}>
      <WorkspaceHeader
        title={title}
        titleRef={titleRef}
        back={
          <Button variant="back" disabled={busy} aria-label={backLabel} onClick={onBack}>
            <span aria-hidden="true">←</span>
          </Button>
        }
        primary={primary}
      />
      <div className="asb-edit-panel">{children}</div>
      {footer && <footer className="asb-editor-footer">{footer}</footer>}
    </div>
  );
}
