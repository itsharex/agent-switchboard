import type { ReactNode, Ref } from "react";

interface WorkspaceHeaderProps {
  /** The workspace's single h2 title; nothing else shares row 1 except the
   * optional sub-workspace back button. */
  title: string;
  /** Optional focus target for sub-workspace route changes. */
  titleRef?: Ref<HTMLHeadingElement>;
  /** Square back icon button ("←") for sub-workspaces, rendered left of the
   * title in row 1. */
  back?: ReactNode;
  /** Row 2 left: section tabs or client navigation. */
  primary?: ReactNode;
  /** Row 2 right: page-level actions, wrapped in the shared actions cluster. */
  primaryActions?: ReactNode;
  /** Row 3: local view controls — search, filters, counts, refresh. */
  secondary?: ReactNode;
}

/**
 * The one page-header structure owner. It always
 * renders row 1 (title alone) and, only when content exists, the primary
 * navigation row and the secondary view-controls row. A missing primary row
 * promotes secondary to the second row so no blank toolbar line renders.
 * `.asb-panel-heading` / `.asb-panel-subheading` are this component's
 * internal implementation; pages assemble neither by hand.
 */
export function WorkspaceHeader({ title, titleRef, back, primary, primaryActions, secondary }: WorkspaceHeaderProps) {
  return (
    <header className="asb-workspace-header">
      <div className="asb-panel-heading">
        <div className="asb-panel-heading-main">
          {back}
          <h2 ref={titleRef} tabIndex={titleRef ? -1 : undefined} className="asb-panel-title">{title}</h2>
        </div>
      </div>
      {(primary || primaryActions) && (
        <div className="asb-panel-subheading asb-header-primary">
          {primary}
          {primaryActions && <div className="asb-panel-actions">{primaryActions}</div>}
        </div>
      )}
      {secondary && (
        <div className="asb-panel-subheading asb-header-secondary">{secondary}</div>
      )}
    </header>
  );
}

interface ModuleHeaderProps {
  /** The module title; always an h3 with the section-title type token. */
  title: string;
  /** Heading id for the enclosing section's aria-labelledby wiring. */
  id?: string;
  /** Row 2 left: module context line. */
  primary?: ReactNode;
  /** Row 2 right: module-level actions. */
  primaryActions?: ReactNode;
  /** Row 3: local view controls. */
  secondary?: ReactNode;
}

/**
 * The embedded-module header: the same fixed row grammar as WorkspaceHeader
 * but with an h3 `.asb-section-title`, so a module inside a page can never
 * grow its own page-level heading.
 */
export function ModuleHeader({ title, id, primary, primaryActions, secondary }: ModuleHeaderProps) {
  return (
    <header className="asb-module-header">
      <div className="asb-panel-heading">
        <div className="asb-panel-heading-main">
          <h3 id={id} className="asb-section-title">{title}</h3>
        </div>
      </div>
      {(primary || primaryActions) && (
        <div className="asb-panel-subheading asb-header-primary">
          {primary}
          {primaryActions && <div className="asb-panel-actions">{primaryActions}</div>}
        </div>
      )}
      {secondary && (
        <div className="asb-panel-subheading asb-header-secondary">{secondary}</div>
      )}
    </header>
  );
}
