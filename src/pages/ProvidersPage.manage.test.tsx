import { expect, it, vi } from "vitest";
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";
import { ProvidersPage } from "./ProvidersPage";
import { answer } from "../test/claude-management";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn(async () => {}) }));
vi.mocked(invoke).mockImplementation(async (command, args) =>
  answer(command, args as Record<string, unknown>),
);

type PageProps = Omit<
  Parameters<typeof ProvidersPage>[0],
  "view" | "onViewChange"
>;

function page(overrides: Partial<PageProps> = {}): PageProps {
  return {
    active: true,
    profiles: [],
    appFilter: "claude",
    activeProfileId: null,
    statuses: null,
    locks: {},
    userConfigModel: null,
    userConfigWarnings: [],
    selectedId: null,
    editorSession: null,
    preview: null,
    busy: false,
    collapsedUsageIds: [],
    onSelectApp: vi.fn(),
    onNew: vi.fn(),
    onImport: vi.fn(),
    onOpenClientSettings: vi.fn(),
    onOpenHistory: vi.fn(),
    onCloseEditor: vi.fn(),
    onSave: vi.fn(),
    onSwitchClient: vi.fn(),
    onSaveUsageQuery: vi.fn(),
    onSaveQuotaInterval: vi.fn(),
    onSelect: vi.fn(),
    onReorder: vi.fn(),
    onToggleUsage: vi.fn(),
    onActivate: vi.fn(),
    onTogglePreview: vi.fn(),
    onEdit: vi.fn(),
    onDelete: vi.fn(),
    onRefresh: vi.fn(),
    ...overrides,
  } as PageProps;
}

it("opens the Claude management dialog directly from the workspace header", async () => {
  const user = userEvent.setup();
  const onRefresh = vi.fn();
  render(
    <ProvidersPage
      {...page({ onRefresh })}
      view={{ kind: "list" }}
      onViewChange={vi.fn()}
    />,
  );
  await user.click(screen.getByRole("button", { name: "管理 Claude 功能" }));
  const dialog = await screen.findByRole("dialog", {
    name: "Claude 本地功能管理",
  });
  expect(within(dialog).getByRole("tab", { name: "供应商" })).toBeInTheDocument();
  expect(
    within(dialog).getByRole("tab", { name: "客户端集成" }),
  ).toBeInTheDocument();
});
