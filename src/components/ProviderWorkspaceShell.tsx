import { CSS } from "@dnd-kit/utilities";
import {
  DndContext,
  KeyboardSensor,
  PointerSensor,
  closestCenter,
  useSensor,
  useSensors,
  type DragEndEvent,
} from "@dnd-kit/core";
import {
  SortableContext,
  arrayMove,
  sortableKeyboardCoordinates,
  useSortable,
  verticalListSortingStrategy,
} from "@dnd-kit/sortable";
import type {
  AppKind,
  ConfigFileStatus,
  LockStatus,
  ProviderProfile,
} from "../api/client";
import type { ReactNode } from "react";
import { Button } from "./Button";
import { ClientPicker } from "./ClientPicker";
import { DualRelay } from "./DualRelay";
import { GripIcon, PlusIcon } from "./icons";
import { Tooltip } from "./Tooltip";
import { WorkspaceHeader } from "./WorkspaceHeader";
import "../styles/base/provider-workspace.css";

interface ProviderWorkspaceShellProps {
  /** Distinguishes the two client views for assistive technology; the visible
   * heading and toolbar are identical on both. */
  ariaLabel: string;
  app: AppKind;
  onSelectApp: (app: AppKind) => void;
  busy: boolean;
  statuses: ConfigFileStatus[] | null;
  profiles: ProviderProfile[];
  locks: Partial<Record<AppKind, LockStatus>>;
  onImport: () => void;
  onNew: () => void;
  /** App-specific actions appended between 导入 and 新建供应商. */
  extraActions?: ReactNode;
  children: ReactNode;
}

/** The one provider-workspace skeleton: the observed connection cards lead
 * the supplier page, followed by the workspace header — row 1 the title
 * alone, row 2 the client switch left and every page action right — and the
 * filtered supplier list. */
export function ProviderWorkspaceShell({
  ariaLabel,
  app,
  onSelectApp,
  busy,
  statuses,
  profiles,
  locks,
  onImport,
  onNew,
  extraActions,
  children,
}: ProviderWorkspaceShellProps) {
  return (
    <section className="asb-provider-workspace" aria-label={ariaLabel}>
      <DualRelay statuses={statuses} profiles={profiles} locks={locks} />
      <section className="asb-panel asb-provider-list-panel" aria-label="供应商列表">
        <WorkspaceHeader
          title="供应商"
          primary={
            <ClientPicker
              app={app}
              onChange={onSelectApp}
              disabled={busy}
              label="供应商客户端"
            />
          }
          primaryActions={
            <>
              <Button variant="secondary" disabled={busy} onClick={onImport}>
                导入
              </Button>
              {extraActions}
              <Button variant="plus" disabled={busy} onClick={onNew}>
                <PlusIcon />
                新建供应商
              </Button>
            </>
          }
        />
        {children}
      </section>
    </section>
  );
}

interface SortableProviderRowsProps {
  ids: string[];
  /** Persists a new display order for the visible rows. */
  onReorder?: (orderedIds: string[]) => void;
  emptyLabel?: string;
  ariaLabel?: string;
  children: ReactNode;
}

/** The one sortable rows container, including the shared empty state. */
export function SortableProviderRows({
  ids,
  onReorder,
  emptyLabel = "尚无供应商",
  ariaLabel = "供应商列表",
  children,
}: SortableProviderRowsProps) {
  const sensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { distance: 8 } }),
    useSensor(KeyboardSensor, { coordinateGetter: sortableKeyboardCoordinates }),
  );
  const handleDragEnd = (event: DragEndEvent) => {
    const { active, over } = event;
    if (!over || active.id === over.id) return;
    const oldIndex = ids.indexOf(String(active.id));
    const newIndex = ids.indexOf(String(over.id));
    if (oldIndex === -1 || newIndex === -1) return;
    onReorder?.(arrayMove(ids, oldIndex, newIndex));
  };
  if (ids.length === 0) {
    return (
      <div className="asb-empty-state">
        <span className="asb-empty-state-icon" aria-hidden="true">
          <PlusIcon />
        </span>
        <h3 className="asb-section-title">{emptyLabel}</h3>
      </div>
    );
  }
  return (
    <ul className="asb-rows" role="list" aria-label={ariaLabel}>
      <DndContext sensors={sensors} collisionDetection={closestCenter} onDragEnd={handleDragEnd}>
        <SortableContext items={ids} strategy={verticalListSortingStrategy}>
          {children}
        </SortableContext>
      </DndContext>
    </ul>
  );
}

interface ProviderRowShellProps {
  id: string;
  name: string;
  active: boolean;
  confirmationOpen: boolean;
  sortable: boolean;
  /** The configured model, rendered in the row's dedicated model zone. */
  model?: ReactNode;
  /** The provider host or official route, rendered in the connection zone. */
  endpoint?: ReactNode;
  /** Usage or account state, rendered beneath the fixed primary data rail. */
  summary?: ReactNode;
  /** Primary action for an inactive profile (启用). */
  primaryAction?: ReactNode;
  /** Secondary explicit action, such as official re-login. */
  secondaryAction?: ReactNode;
  /** Secondary icon actions; hidden until hover or keyboard focus on pointer devices. */
  actions?: ReactNode;
  /** Expansion blocks rendered below the data row. */
  children?: ReactNode;
}

/**
 * One provider row keeps identity, model, connection, and actions on a
 * fixed scan rail. Its optional full-width summary lives below that rail,
 * leaving the model value as an unambiguous single-line fact.
 */
export function ProviderRowShell({
  id,
  name,
  active,
  confirmationOpen,
  sortable,
  model,
  endpoint,
  summary,
  primaryAction,
  secondaryAction,
  actions,
  children,
}: ProviderRowShellProps) {
  const { setNodeRef, attributes, listeners, transform, transition, isDragging } = useSortable({
    id,
    disabled: !sortable,
  });
  const initial = name.trim().charAt(0).toUpperCase() || "?";
  const hasSummary = summary !== undefined && summary !== null;
  return (
    <li
      ref={setNodeRef}
      style={{ transform: CSS.Transform.toString(transform), transition }}
      className={`asb-row-item${active ? " is-live" : ""}${isDragging ? " is-dragging" : ""}${confirmationOpen ? " is-confirming" : ""}`}
    >
      <div className="asb-row-line">
        {sortable ? (
          <Tooltip label={`拖动调整 ${name} 的顺序`}>
            <Button
              variant="unstyled"
              className="asb-row-grip"
              aria-label={`拖动调整 ${name} 的顺序`}
              {...attributes}
              {...listeners}
            >
              <GripIcon />
            </Button>
          </Tooltip>
        ) : <span className="asb-row-grip-spacer" aria-hidden="true" />}
        <div className="asb-row">
          <span className="asb-avatar" aria-hidden="true">{initial}</span>
          <span className="asb-row-identity-copy">
            <span className="asb-row-identity-heading">
              <span className="asb-row-name" title={name}>{name}</span>
              {active && <span className="asb-pill-status">已应用</span>}
            </span>
          </span>
        </div>
        <span className="asb-row-model">
          <span className="asb-row-model-value">{model ?? "默认模型"}</span>
        </span>
        <span className="asb-row-endpoint">{endpoint}</span>
        <span className="asb-row-controls">
          {primaryAction}
          {secondaryAction}
          {actions && <span className="asb-iconcluster" role="group" aria-label={`${name} 操作`}>{actions}</span>}
        </span>
      </div>
      {hasSummary && <div className="asb-row-summary">{summary}</div>}
      {children}
    </li>
  );
}
