import { useState, type ComponentProps } from "react";
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it, vi } from "vitest";
import type { SettingsSection } from "../app/navigation";
import { SettingsPage } from "./SettingsPage";
import { BackupsPage } from "./BackupsPage";

type PageProps = ComponentProps<typeof SettingsPage>;
const defaults: Omit<PageProps, "section" | "onSectionChange"> = {
  clientSettings: (
    <label>
      客户端草稿
      <input defaultValue="已保留的偏好" />
    </label>
  ),
  backups: <section aria-label="备份内容">备份列表</section>,
  diagnostics: <section aria-label="诊断内容">配置与环境</section>,
  settings: null,
  loadError: null,
  busy: false,
  onRetryLoad: vi.fn(),
  onRepair: vi.fn(),
  onPatch: vi.fn(),
  onRestart: vi.fn(),
  updateCheck: null,
  updateChannel: null,
  appVersion: "0.1.5",
  updateChecking: false,
  updateInstalling: false,
  updateProgress: null,
  updateCheckedAt: null,
  updateRestartRequired: false,
  onCheckUpdate: vi.fn(),
  onInstallUpdate: vi.fn(),
  onRestartInstalledUpdate: vi.fn(),
};

function SettingsHarness({
  initial = "application",
  ...props
}: Partial<Omit<PageProps, "section" | "onSectionChange">> & {
  initial?: SettingsSection;
}) {
  const [section, setSection] = useState<SettingsSection>(initial);
  return (
    <SettingsPage
      {...defaults}
      {...props}
      section={section}
      onSectionChange={setSection}
    />
  );
}

it("uses a controlled, named settings navigation with a direct client destination", async () => {
  const user = userEvent.setup();
  const onSectionChange = vi.fn();
  const { rerender } = render(
    <SettingsPage
      {...defaults}
      section="client"
      onSectionChange={onSectionChange}
    />,
  );

  const navigation = screen.getByRole("navigation", { name: "设置分类" });
  expect(
    within(navigation)
      .getAllByRole("button")
      .map((button) => button.textContent),
  ).toEqual(["应用偏好", "偏好设置", "备份与恢复", "诊断", "关于与更新"]);
  expect(
    within(navigation).getByRole("button", { name: "偏好设置" }),
  ).toHaveAttribute("aria-current", "page");
  expect(screen.getByRole("textbox", { name: "客户端草稿" })).toBeVisible();
  expect(screen.queryByRole("tablist")).not.toBeInTheDocument();

  await user.click(within(navigation).getByRole("button", { name: "诊断" }));
  expect(onSectionChange).toHaveBeenCalledExactlyOnceWith("diagnostics");
  rerender(
    <SettingsPage
      {...defaults}
      section="diagnostics"
      onSectionChange={onSectionChange}
    />,
  );
  expect(screen.getByRole("region", { name: "诊断内容" })).toBeVisible();
  expect(
    screen.queryByRole("textbox", { name: "客户端草稿" }),
  ).not.toBeInTheDocument();
});

it("keeps the client draft mounted across settings categories and puts updates under about", async () => {
  const user = userEvent.setup();
  const onCheckUpdate = vi.fn();
  render(<SettingsHarness initial="client" onCheckUpdate={onCheckUpdate} />);

  const draft = screen.getByRole("textbox", { name: "客户端草稿" });
  await user.type(draft, "，继续编辑");
  await user.click(screen.getByRole("button", { name: "应用偏好" }));
  expect(
    screen.queryByRole("button", { name: "检查更新" }),
  ).not.toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "关于与更新" }));
  expect(screen.getByText("当前版本 v0.1.5")).toBeVisible();
  await user.click(screen.getByRole("button", { name: "检查更新" }));
  expect(onCheckUpdate).toHaveBeenCalledOnce();
  await user.click(screen.getByRole("button", { name: "偏好设置" }));
  expect(screen.getByRole("textbox", { name: "客户端草稿" })).toBe(draft);
  expect(draft).toHaveValue("已保留的偏好，继续编辑");
});

it("offers a return to the provider workspace for directed settings entry", async () => {
  const user = userEvent.setup();
  const onReturnToProviders = vi.fn();
  render(
    <SettingsHarness
      initial="client"
      onReturnToProviders={onReturnToProviders}
    />,
  );

  await user.click(screen.getByRole("button", { name: "返回供应商" }));
  expect(onReturnToProviders).toHaveBeenCalledOnce();
});

it("keeps the existing backup confirmation and restore reachable from settings", async () => {
  const user = userEvent.setup();
  const onRestore = vi.fn();
  const onUndo = vi.fn();
  render(
    <SettingsHarness
      backups={
        <BackupsPage
          busy={false}
          onRestore={onRestore}
          onUndo={onUndo}
          onOpenDir={vi.fn()}
          records={[
            {
              id: "backup-one",
              app: "codex",
              targetPath: "/isolated/config.toml",
              backupPath: "/isolated/backup.toml",
              createdAt: "2026-09-08T00:00:00Z",
              contentHash: "fixture-config-hash",
              targetExisted: true,
              linkedBackupId: null,
              reason: "switch",
            },
          ]}
          lastSwitch={{
            app: "codex",
            profileId: "gateway",
            profileName: "网关",
            contentHash: "fixture-config-hash",
            backupId: "backup-one",
            at: "2026-09-08T00:00:00Z",
            operation: "projection",
          }}
          cloudBackup={{
            settings: null,
            loaded: true,
            saveSettings: vi.fn(),
            testConnection: vi.fn(),
            upload: vi.fn(),
            restore: vi.fn(),
          }}
        />
      }
    />,
  );

  await user.click(screen.getByRole("button", { name: "备份与恢复" }));
  expect(screen.getByRole("table", { name: "备份历史" })).toBeVisible();
  await user.click(screen.getByRole("button", { name: "恢复" }));
  expect(onRestore).not.toHaveBeenCalled();
  await user.click(screen.getByRole("button", { name: "确认恢复" }));
  expect(onRestore).toHaveBeenCalledExactlyOnceWith("backup-one");
  await user.click(screen.getByRole("button", { name: "撤回上一次切换" }));
  expect(onUndo).toHaveBeenCalledOnce();
});

it("retains load repair actions in application preferences", async () => {
  const user = userEvent.setup();
  const onRepair = vi.fn();
  const onRetryLoad = vi.fn();
  render(
    <SettingsHarness
      loadError="配置文件不可读"
      onRepair={onRepair}
      onRetryLoad={onRetryLoad}
    />,
  );

  expect(screen.getByRole("alert")).toHaveTextContent(
    "设置加载失败：配置文件不可读",
  );
  await user.click(screen.getByRole("button", { name: "重试" }));
  expect(onRetryLoad).toHaveBeenCalledOnce();
  await user.click(screen.getByRole("button", { name: "一键修复" }));
  expect(onRepair).toHaveBeenCalledOnce();
});
