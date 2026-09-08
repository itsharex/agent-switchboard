import { useState } from "react";
import { describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Tabs } from "./Tabs";

type TabValue = "a" | "b" | "c";

const THREE_TABS = [
  { value: "a" as const, label: "甲" },
  { value: "b" as const, label: "乙", disabled: true },
  { value: "c" as const, label: "丙" },
];

function Harness({
  tabs,
  onChange,
}: {
  tabs: ReadonlyArray<{ value: TabValue; label: string; disabled?: boolean }>;
  onChange?: (value: TabValue) => void;
}) {
  const [value, setValue] = useState<TabValue>("a");
  return (
    <Tabs
      value={value}
      onChange={(next) => {
        onChange?.(next);
        setValue(next);
      }}
      scope="test"
      label="测试"
      tabs={tabs}
    />
  );
}

describe("Tabs keyboard navigation", () => {
  it("skips a disabled tab when moving with the arrow keys", async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    render(<Harness tabs={THREE_TABS} onChange={onChange} />);

    const first = screen.getByRole("tab", { name: "甲" });
    first.focus();
    await user.keyboard("{ArrowRight}");

    expect(onChange).toHaveBeenCalledWith("c");
    expect(screen.getByRole("tab", { name: "丙" })).toHaveFocus();

    await user.keyboard("{ArrowLeft}");
    expect(onChange).toHaveBeenLastCalledWith("a");
    expect(screen.getByRole("tab", { name: "甲" })).toHaveFocus();
  });

  it("resolves Home and End to the first and last enabled tabs", async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    render(<Harness tabs={THREE_TABS} onChange={onChange} />);

    screen.getByRole("tab", { name: "甲" }).focus();
    await user.keyboard("{End}");
    expect(onChange).toHaveBeenCalledWith("c");
    expect(screen.getByRole("tab", { name: "丙" })).toHaveFocus();

    await user.keyboard("{Home}");
    expect(onChange).toHaveBeenLastCalledWith("a");
    expect(screen.getByRole("tab", { name: "甲" })).toHaveFocus();
  });

  it("wraps around the enabled tabs in both directions", async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    render(<Harness tabs={THREE_TABS} onChange={onChange} />);

    screen.getByRole("tab", { name: "丙" }).focus();
    await user.keyboard("{ArrowRight}");
    expect(onChange).toHaveBeenLastCalledWith("a");
    expect(screen.getByRole("tab", { name: "甲" })).toHaveFocus();

    await user.keyboard("{ArrowLeft}");
    expect(onChange).toHaveBeenLastCalledWith("c");
    expect(screen.getByRole("tab", { name: "丙" })).toHaveFocus();
  });

  it("leaves the selection untouched when no other tab is enabled", async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    render(
      <Harness
        tabs={[
          { value: "a", label: "甲" },
          { value: "b", label: "乙", disabled: true },
          { value: "c", label: "丙", disabled: true },
        ]}
        onChange={onChange}
      />,
    );

    screen.getByRole("tab", { name: "甲" }).focus();
    await user.keyboard("{ArrowRight}{ArrowLeft}{Home}{End}");

    expect(onChange).not.toHaveBeenCalled();
    expect(screen.getByRole("tab", { name: "甲" })).toHaveFocus();
    expect(screen.getByRole("tab", { name: "甲" })).toHaveAttribute("aria-selected", "true");
  });
});
