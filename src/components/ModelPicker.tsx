import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type RefObject,
} from "react";
import { createPortal } from "react-dom";
import { useI18n } from "../i18n";
import { Input } from "./Input";
import { Button } from "./Button";
import { CheckIcon, ChevronDownIcon, SearchIcon } from "./icons";

/** Group label key for models the endpoint did not attribute to a vendor;
 * translated at render so the language switch relabels the fallback group. */
const OTHER_GROUP_KEY = "providers.models.otherGroup";

/** Menu offsets in px: the trigger gap mirrors --asb-space-unit, the viewport
 * margin keeps the fixed menu inside the window on every side. */
const TRIGGER_GAP = 4;
const VIEWPORT_MARGIN = 8;

export interface ModelPickerOption {
  value: string;
  label: string;
  group: string | null;
}

interface Group {
  vendor: string;
  models: ModelPickerOption[];
}

/** Vendor-grouped models sorted for browsing; `needle` matches either the
 * model id or its vendor, case-insensitively. */
function groupModels(models: ModelPickerOption[], needle: string, otherGroup: string): Group[] {
  const groups = new Map<string, ModelPickerOption[]>();
  for (const model of models) {
    const vendor = model.group ?? otherGroup;
    if (!groups.has(vendor)) groups.set(vendor, []);
    groups.get(vendor)!.push(model);
  }
  return [...groups.entries()]
    .map(([vendor, vendorModels]) => ({
      vendor,
      models: vendorModels
        .filter(
          (model) =>
            model.label.toLocaleLowerCase().includes(needle) ||
            vendor.toLocaleLowerCase().includes(needle),
        )
        .sort((a, b) => a.label.localeCompare(b.label)),
    }))
    .filter((group) => group.models.length > 0)
    // The unattributed fallback group stays last regardless of locale.
    .sort((a, b) =>
      a.vendor === otherGroup ? 1 : b.vendor === otherGroup ? -1 : a.vendor.localeCompare(b.vendor),
    );
}

interface Props {
  models: ModelPickerOption[];
  /** Stable option value, independent of the displayed model label. */
  current: string | null;
  /** Accessible name of the trigger and the search field. */
  ariaLabel: string;
  disabled?: boolean;
  onSelect: (id: string) => void;
}

interface MenuPlacement {
  top: number;
  left: number;
}

/** Fixed coordinates that keep the menu attached to its trigger: right-aligned
 * like the in-flow menu it replaces, flipped above when the window edge is
 * nearer, and clamped so the menu never leaves the viewport. */
function placeMenu(trigger: DOMRect, menu: HTMLElement, flipped: boolean): MenuPlacement {
  const width = menu.offsetWidth;
  const height = menu.offsetHeight;
  const left = Math.min(
    Math.max(trigger.right - width, VIEWPORT_MARGIN),
    Math.max(window.innerWidth - width - VIEWPORT_MARGIN, VIEWPORT_MARGIN),
  );
  const top = flipped
    ? trigger.top - TRIGGER_GAP - height
    : trigger.bottom + TRIGGER_GAP;
  const topLimit = Math.max(window.innerHeight - height - VIEWPORT_MARGIN, VIEWPORT_MARGIN);
  return { top: Math.min(Math.max(top, VIEWPORT_MARGIN), topLimit), left };
}

/** Anchoring behavior for the portaled menu: decide the side once per opening
 * so scrolling never makes it jump, then follow trigger moves (scroll in any
 * container, window resize) while staying attached and inside the window. */
function useAnchoredMenuPlacement(
  open: boolean,
  trigger: RefObject<HTMLButtonElement | null>,
  menu: RefObject<HTMLDivElement | null>,
): MenuPlacement | null {
  const [placement, setPlacement] = useState<MenuPlacement | null>(null);
  const flipped = useRef(false);

  const sync = useCallback(() => {
    const triggerRect = trigger.current?.getBoundingClientRect();
    const menuNode = menu.current;
    if (!triggerRect || !menuNode) return;
    setPlacement((previous) => {
      const next = placeMenu(triggerRect, menuNode, flipped.current);
      return previous?.top === next.top && previous?.left === next.left ? previous : next;
    });
  }, [trigger, menu]);

  useLayoutEffect(() => {
    if (!open) {
      setPlacement(null);
      return;
    }
    const triggerRect = trigger.current?.getBoundingClientRect();
    const menuNode = menu.current;
    if (!triggerRect || !menuNode) return;
    const spaceBelow = window.innerHeight - triggerRect.bottom - TRIGGER_GAP;
    const spaceAbove = triggerRect.top - TRIGGER_GAP;
    flipped.current = menuNode.offsetHeight > spaceBelow && spaceAbove > spaceBelow;
    sync();
  }, [open, sync, trigger, menu]);

  useEffect(() => {
    if (!open) return;
    // Capture: the page and inner form containers scroll, the window does not.
    window.addEventListener("scroll", sync, true);
    window.addEventListener("resize", sync);
    return () => {
      window.removeEventListener("scroll", sync, true);
      window.removeEventListener("resize", sync);
    };
  }, [open, sync]);

  return placement;
}

interface OptionGroupsProps {
  groups: Group[];
  current: string | null;
  onSelect: (id: string) => void;
}

/** The vendor-grouped listbox body; each row reserves a trailing check slot so
 * the current item's mark never shifts the name column. */
function ModelOptionGroups({ groups, current, onSelect }: OptionGroupsProps) {
  return (
    <>
      {groups.map((group) => (
        <div key={group.vendor} role="group" aria-label={group.vendor}>
          <p className="asb-model-group">{group.vendor}</p>
          {group.models.map((model) => (
            <Button
              variant="unstyled"
              role="option"
              aria-selected={model.value === current}
              className="asb-model-option"
              key={model.value}
              onClick={() => onSelect(model.value)}
            >
              <span className="asb-model-option-name">{model.label}</span>
              <span className="asb-model-check" aria-hidden="true">
                {model.value === current && <CheckIcon />}
              </span>
            </Button>
          ))}
        </div>
      ))}
    </>
  );
}

interface ModelMenuProps {
  models: ModelPickerOption[];
  current: string | null;
  ariaLabel: string;
  placement: MenuPlacement | null;
  /** Shared with the picker, which measures the menu to pin it to the trigger. */
  menuRef: RefObject<HTMLDivElement | null>;
  onSelect: (id: string) => void;
  onClose: () => void;
}

/** The portaled menu body: search field plus the vendor-grouped listbox. Its
 * query dies with the portal, and the keyboard contract matches FontPicker. */
function ModelMenu({
  models,
  current,
  ariaLabel,
  placement,
  menuRef,
  onSelect,
  onClose,
}: ModelMenuProps) {
  const { t } = useI18n();
  const [query, setQuery] = useState("");
  const options = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const selected = options.current?.querySelector<HTMLButtonElement>(
      'button[aria-selected="true"]',
    );
    // jsdom has no scrollIntoView; the visual nicety is optional.
    if (typeof selected?.scrollIntoView === "function") {
      selected.scrollIntoView({ block: "nearest" });
    }
  }, []);

  const onMenuKeyDown = (event: React.KeyboardEvent) => {
    if (event.key === "Escape") {
      event.preventDefault();
      onClose();
      return;
    }
    const buttons = Array.from(
      options.current?.querySelectorAll<HTMLButtonElement>('button[role="option"]') ?? [],
    );
    const active = buttons.findIndex((button) => button === document.activeElement);
    const delta = event.key === "ArrowDown" ? 1 : event.key === "ArrowUp" ? -1 : 0;
    if (delta !== 0 || event.key === "Home" || event.key === "End") {
      event.preventDefault();
      let next: number;
      if (event.key === "Home") next = 0;
      else if (event.key === "End") next = buttons.length - 1;
      else next = active + delta;
      if (next >= 0 && next < buttons.length) buttons[next].focus();
    }
  };

  const needle = query.trim().toLocaleLowerCase();
  const groups = useMemo(() => groupModels(models, needle, t(OTHER_GROUP_KEY)), [models, needle, t]);

  return (
    <div
      className="asb-model-menu"
      ref={menuRef}
      style={placement ?? undefined}
      onKeyDown={onMenuKeyDown}
    >
      <label className="asb-model-search">
        <SearchIcon />
        <Input
          type="search"
          aria-label={t("providers.models.searchAria", { label: ariaLabel })}
          placeholder={t("providers.models.searchPlaceholder")}
          value={query}
          autoFocus
          onChange={(event) => setQuery(event.target.value)}
        />
      </label>
      <div
        className="asb-model-options"
        ref={options}
        role="listbox"
        aria-label={ariaLabel}
      >
        {groups.length > 0 ? (
          <ModelOptionGroups groups={groups} current={current} onSelect={onSelect} />
        ) : (
          <p className="asb-model-empty">{t("providers.models.empty")}</p>
        )}
      </div>
    </div>
  );
}

/**
 * A field-shaped icon trigger opens a searchable menu whose models are grouped by vendor, so a
 * fetched list stays browsable at aggregate-relay scale. The menu portals to <body> and is
 * fixed-anchored to its trigger: editor containers can no longer clip it or make it float
 * away from the field it belongs to, it flips up near the window bottom, and its width grows
 * to fit long model ids. Interaction contract (keyboard, close behavior, material) matches
 * the FontPicker listbox.
 */
export function ModelPicker({ models, current, ariaLabel, disabled = false, onSelect }: Props) {
  const [open, setOpen] = useState(false);
  const root = useRef<HTMLDivElement>(null);
  const trigger = useRef<HTMLButtonElement>(null);
  const menu = useRef<HTMLDivElement>(null);
  const placement = useAnchoredMenuPlacement(open, trigger, menu);

  const close = (refocus = true) => {
    setOpen(false);
    if (refocus) trigger.current?.focus();
  };

  const selectModel = (id: string) => {
    onSelect(id);
    close();
  };

  useEffect(() => {
    if (!open) return;
    const onPointerDown = (event: PointerEvent) => {
      const target = event.target as Node;
      if (!root.current?.contains(target) && !menu.current?.contains(target)) {
        close();
      }
    };
    document.addEventListener("pointerdown", onPointerDown);
    return () => document.removeEventListener("pointerdown", onPointerDown);
  }, [open]);

  useEffect(() => {
    if (disabled) setOpen(false);
  }, [disabled]);

  return (
    <div
      className="asb-model-picker"
      ref={root}
      onKeyDown={(event) => {
        if (event.key === "Escape" && open) {
          event.preventDefault();
          close();
        }
      }}
    >
      <Button
        variant="unstyled"
        ref={trigger}
        className="asb-model-trigger"
        aria-haspopup="listbox"
        aria-expanded={open && !disabled}
        aria-label={ariaLabel}
        disabled={disabled}
        onClick={() => (open ? close(false) : setOpen(true))}
      >
        <span className="asb-select-chevron" aria-hidden="true">
          <ChevronDownIcon />
        </span>
      </Button>

      {open && !disabled
        ? createPortal(
            <ModelMenu
              models={models}
              current={current}
              ariaLabel={ariaLabel}
              placement={placement}
              menuRef={menu}
              onSelect={selectModel}
              onClose={() => close()}
            />,
            document.body,
          )
        : null}
    </div>
  );
}
