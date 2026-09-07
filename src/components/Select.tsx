import * as SelectPrimitive from "@radix-ui/react-select";
import { CheckIcon, ChevronDownIcon, ChevronUpIcon } from "./icons";

export interface SelectOption {
  value: string;
  label: string;
}

interface Props {
  /** Currently chosen option value; null = nothing chosen yet. */
  value: string | null;
  options: readonly SelectOption[];
  /** Emits the newly chosen option value. */
  onChange: (value: string) => void;
  /** Trigger text while no value is set. */
  placeholder?: string;
  /** Accessible name of the combobox trigger. */
  ariaLabel: string;
  disabled?: boolean;
}

/**
 * Dropdown picker:
 * field-shaped trigger with a trailing chevron, frosted-menu popover with
 * the chosen item marked by a right-aligned check. Rendering contract and
 * visuals are owned here; all values come from styles/tokens.css.
 */
export function Select({
  value,
  options,
  onChange,
  placeholder,
  ariaLabel,
  disabled = false,
}: Props) {
  return (
    <SelectPrimitive.Root value={value ?? ""} onValueChange={onChange} disabled={disabled}>
      <SelectPrimitive.Trigger className="asb-select-trigger" aria-label={ariaLabel}>
        <SelectPrimitive.Value placeholder={placeholder} />
        <SelectPrimitive.Icon className="asb-select-chevron">
          <ChevronDownIcon />
        </SelectPrimitive.Icon>
      </SelectPrimitive.Trigger>
      <SelectPrimitive.Portal>
        <SelectPrimitive.Content className="asb-select-content" position="popper" sideOffset={4}>
          <SelectPrimitive.ScrollUpButton className="asb-select-scroll">
            <ChevronUpIcon />
          </SelectPrimitive.ScrollUpButton>
          <SelectPrimitive.Viewport className="asb-select-viewport">
            {options.map(({ value: optionValue, label }) => (
              <SelectPrimitive.Item key={optionValue} value={optionValue} className="asb-select-item">
                <SelectPrimitive.ItemText>{label}</SelectPrimitive.ItemText>
                <span className="asb-select-check" aria-hidden="true">
                  <SelectPrimitive.ItemIndicator>
                    <CheckIcon />
                  </SelectPrimitive.ItemIndicator>
                </span>
              </SelectPrimitive.Item>
            ))}
          </SelectPrimitive.Viewport>
          <SelectPrimitive.ScrollDownButton className="asb-select-scroll">
            <ChevronDownIcon />
          </SelectPrimitive.ScrollDownButton>
        </SelectPrimitive.Content>
      </SelectPrimitive.Portal>
    </SelectPrimitive.Root>
  );
}
