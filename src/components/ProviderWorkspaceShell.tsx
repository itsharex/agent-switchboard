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
import { cx } from "@/utils/cx";
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
  onOpenClientSettings: () => void;
  onOpenHistory: () => void;
  onImport: () => void;
  onNew: () => void;
  /** App-specific actions appended between 导入 and 新建供应商. */
  extraActions?: ReactNode;
  children: ReactNode;
}

/** The one provider-workspace skeleton (2026-09-12 user directive; the two
 * clients render identically since 2026-09-15): the workspace header — row 1
 * the title alone, row 2 the client switch left and every page action right —
 * followed by the dual route cards and the lists. */
export function ProviderWorkspaceShell({
  ariaLabel,
  app,
  onSelectApp,
  busy,
  statuses,
  profiles,
  locks,
  onOpenClientSettings,
  onOpenHistory,
  onImport,
  onNew,
  extraActions,
  children,
}: ProviderWorkspaceShellProps) {
  return (
    <section className="asb-panel asb-provider-workspace" aria-label={ariaLabel}>
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
            <Button variant="secondary" onClick={onOpenClientSettings}>
              偏好设置
            </Button>
            <Button variant="secondary" onClick={onOpenHistory}>
              切换历史
            </Button>
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
      <DualRelay statuses={statuses} profiles={profiles} locks={locks} />
      {children}
    </section>
  );
}

interface SortableProviderRowsProps {
  ids: string[];
  /** Persists a new display order for the visible rows. */
  onReorder?: (orderedIds: string[]) => void;
  emptyLabel?: string;
  ariaLabel?: string;
  /** A fixed row rendered above the sortable ones (e.g. the Codex
   * official-login entry). It is never a reorder target. */
  leading?: ReactNode;
  children: ReactNode;
}

/** The one sortable rows container, including the shared empty state. */
export function SortableProviderRows({
  ids,
  onReorder,
  emptyLabel = "尚无供应商",
  ariaLabel = "供应商列表",
  leading,
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
  if (ids.length === 0 && !leading) {
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
          {leading}
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
  selected: boolean;
  previewOpen: boolean;
  sortable: boolean;
  /** Detail line inside the row; omitted entirely when null. */
  meta?: ReactNode;
  metaWithUsage?: boolean;
  /** Primary action before the status pill (启用). */
  primaryAction?: ReactNode;
  /** Action after the status pill (official 重新登录). */
  secondaryAction?: ReactNode;
  /** Icon-cluster actions; the cluster renders only when non-null. */
  actions?: ReactNode;
  /** Expansion blocks rendered under the row line (panels, inline preview). */
  children?: ReactNode;
}

/** The one provider row layout: grip, avatar, name, meta line, primary
 * action, status pill, and the icon cluster. Both clients compose their rows
 * on this shell so the list UI stays a single visual language. The identity
 * bar (avatar, name, meta) is display-only (2026-09-12 user directive): only
 * the action buttons are clickable, and the pages select the row from those
 * buttons so the bar keeps its selected highlight afterwards. */
export function ProviderRowShell({
  id,
  name,
  active,
  selected,
  previewOpen,
  sortable,
  meta,
  metaWithUsage = false,
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
  const identity = (
    <>
      <span className="asb-avatar" aria-hidden="true">
        {initial}
      </span>
      <span className="asb-row-main">
        <span className="asb-row-name">{name}</span>
        {meta && (
          <span className={cx("asb-row-meta", metaWithUsage && "asb-row-meta-with-usage")}>
            {meta}
          </span>
        )}
      </span>
    </>
  );
  return (
    <li
      ref={setNodeRef}
      style={{ transform: CSS.Transform.toString(transform), transition }}
      className={`asb-row-item${active ? " is-live" : ""}${selected ? " is-selected" : ""}${isDragging ? " is-dragging" : ""}${previewOpen ? " is-previewing" : ""}`}
    >
      <div className="asb-row-line">
        {sortable && (
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
        )}
        <div className="asb-row">{identity}</div>        {primaryAction}
        {active && <span className="asb-pill-status">使用中</span>}
        {secondaryAction}
        {actions && (
          <span className="asb-iconcluster" role="group" aria-label={`${name} 操作`}>
            {actions}
          </span>
        )}
      </div>
      {children}
    </li>
  );
}
