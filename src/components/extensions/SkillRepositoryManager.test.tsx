import {
  candidate, deferred, otherRepository, refreshCatalog, repository, sourceApi, sourceProps,
} from "./skill-source-test-support";
import { expect, it } from "vitest";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { openUrl } from "@tauri-apps/plugin-opener";
import type { SkillRepository } from "../../api/extensions/skill-sources";
import { SkillSourceBrowser } from "./SkillSourceBrowser";

async function openManager(user: ReturnType<typeof userEvent.setup>) {
  await user.click(screen.getByRole("button", { name: "管理仓库" }));
  const manager = screen.getByRole("region", { name: "Skill 仓库" });
  await waitFor(() => expect(within(manager).getByRole("button", { name: "添加仓库" })).toBeEnabled());
  return manager;
}

it("adds a saved repository URL with optional branch and subpath without fetching it", async () => {
  const user = userEvent.setup();
  render(<SkillSourceBrowser {...sourceProps()} />);
  const dialog = await openManager(user);
  await user.click(within(dialog).getByRole("button", { name: "添加仓库" }));
  const form = within(dialog).getByRole("form", { name: "添加 Skill 仓库" });
  await user.type(within(form).getByRole("textbox", { name: "GitHub 仓库" }), "https://github.com/new/skills.git");
  await user.type(within(form).getByRole("textbox", { name: "分支或提交（可选）" }), "feature/skills");
  await user.type(within(form).getByRole("textbox", { name: "Skill 子目录（可选）" }), "plugins/skills");
  await user.click(within(form).getByRole("button", { name: "保存仓库" }));
  expect(sourceApi.saveSkillRepository).toHaveBeenCalledWith({
    repo: "new/skills", refName: "feature/skills", subpath: "plugins/skills", enabled: true,
  });
  expect(within(dialog).getByRole("listitem", { name: "new/skills" })).toHaveTextContent("feature/skills · plugins/skills");
  expect(sourceApi.scanSkillRepositories).not.toHaveBeenCalled();
  expect(sourceApi.searchSkillDirectory).not.toHaveBeenCalled();
});

it("edits a repository in place and invalidates only that repository's scanned state", async () => {
  sourceApi.listSkillRepositories.mockResolvedValue([repository, otherRepository]);
  const user = userEvent.setup();
  render(<SkillSourceBrowser {...sourceProps()} />);
  await refreshCatalog(user);
  const dialog = await openManager(user);
  await user.click(within(dialog).getByRole("button", { name: "编辑仓库 example/skills" }));
  const form = within(dialog).getByRole("form", { name: "编辑 Skill 仓库" });
  expect(within(form).getByRole("textbox", { name: "GitHub 仓库" })).toHaveValue(repository.repo);
  await user.type(within(form).getByRole("textbox", { name: "分支或提交（可选）" }), "next");
  await user.click(within(form).getByRole("button", { name: "保存仓库" }));
  expect(sourceApi.saveSkillRepository).toHaveBeenCalledWith({ ...repository, refName: "next" });
  expect(within(dialog).getByRole("listitem", { name: repository.repo })).toHaveTextContent("未扫描");
  expect(within(dialog).getByRole("listitem", { name: otherRepository.repo })).toHaveTextContent("0 个 Skills");
  expect(sourceApi.scanSkillRepositories).toHaveBeenCalledOnce();
  await user.click(within(dialog).getByRole("button", { name: "返回发现" }));
  expect(screen.queryByRole("listitem", { name: candidate.name })).not.toBeInTheDocument();
});

it("shows honest unscanned and refreshed counts, and opens a canonical source link", async () => {
  const user = userEvent.setup();
  render(<SkillSourceBrowser {...sourceProps()} />);
  let dialog = await openManager(user);
  expect(within(dialog).getByRole("listitem", { name: repository.repo })).toHaveTextContent("未扫描");
  await user.click(within(dialog).getByRole("link", { name: "查看仓库 example/skills" }));
  expect(openUrl).toHaveBeenCalledWith("https://github.com/example/skills");
  await user.click(within(dialog).getByRole("button", { name: "返回发现" }));
  await refreshCatalog(user);
  dialog = await openManager(user);
  expect(within(dialog).getByRole("listitem", { name: repository.repo })).toHaveTextContent("2 个 Skills");
});

it("enables and disables saved repositories without automatically scanning", async () => {
  const user = userEvent.setup();
  render(<SkillSourceBrowser {...sourceProps()} />);
  const dialog = await openManager(user);
  await user.click(within(dialog).getByRole("checkbox", { name: "启用仓库 example/skills" }));
  expect(sourceApi.saveSkillRepository).toHaveBeenCalledWith({ ...repository, enabled: false });
  expect(within(dialog).getByRole("checkbox", { name: "启用仓库 example/skills" })).not.toBeChecked();
  await user.click(within(dialog).getByRole("button", { name: "返回发现" }));
  expect(screen.getByRole("button", { name: "刷新仓库" })).toBeDisabled();
  expect(screen.getByText("1 个仓库 · 0 个启用")).toBeInTheDocument();
  expect(sourceApi.scanSkillRepositories).not.toHaveBeenCalled();
});

it("removes a repository only after confirmation without changing installed Skills", async () => {
  const callbacks = sourceProps();
  const user = userEvent.setup();
  render(<SkillSourceBrowser {...callbacks} />);
  const dialog = await openManager(user);
  await user.click(within(dialog).getByRole("button", { name: "移除仓库 example/skills" }));
  expect(sourceApi.removeSkillRepository).not.toHaveBeenCalled();
  expect(within(dialog).getByText("仅移除发现来源，已安装的 Skill 保持不变。")).toBeInTheDocument();
  await user.click(within(dialog).getByRole("button", { name: "确认移除仓库" }));
  expect(sourceApi.removeSkillRepository).toHaveBeenCalledWith(repository.id);
  expect(within(dialog).queryByRole("listitem", { name: repository.repo })).not.toBeInTheDocument();
  expect(callbacks.onImport).not.toHaveBeenCalled();
  expect(sourceApi.scanSkillRepositories).not.toHaveBeenCalled();
});

it("preserves the edit draft and catalog on a save failure and retries without duplicate writes", async () => {
  const pending = deferred<SkillRepository>();
  sourceApi.saveSkillRepository.mockReturnValueOnce(pending.promise);
  const user = userEvent.setup();
  render(<SkillSourceBrowser {...sourceProps()} />);
  const dialog = await openManager(user);
  await user.click(within(dialog).getByRole("button", { name: "编辑仓库 example/skills" }));
  const form = within(dialog).getByRole("form", { name: "编辑 Skill 仓库" });
  await user.type(within(form).getByRole("textbox", { name: "分支或提交（可选）" }), "release");
  fireEvent.submit(form); fireEvent.submit(form);
  expect(sourceApi.saveSkillRepository).toHaveBeenCalledOnce();
  expect(within(dialog).getByRole("button", { name: "返回发现" })).toBeDisabled();
  await act(async () => pending.reject(new Error("仓库配置暂不可写")));
  expect(within(dialog).getByRole("alert")).toHaveTextContent("仓库配置暂不可写");
  expect(within(form).getByRole("textbox", { name: "分支或提交（可选）" })).toHaveValue("release");
  await user.click(within(form).getByRole("button", { name: "保存仓库" }));
  expect(sourceApi.saveSkillRepository).toHaveBeenCalledTimes(2);
  expect(within(dialog).queryByRole("form")).not.toBeInTheDocument();
  expect(within(dialog).getByRole("listitem", { name: repository.repo })).toHaveTextContent("release");
});

it("retries a failed local catalog load without any remote request", async () => {
  sourceApi.listSkillRepositories.mockRejectedValueOnce(new Error("仓库目录配置无法读取"));
  const user = userEvent.setup();
  render(<SkillSourceBrowser {...sourceProps()} />);
  expect(await screen.findByRole("alert")).toHaveTextContent("仓库目录配置无法读取");
  await user.click(screen.getByRole("button", { name: "重试加载仓库" }));
  expect(screen.getByRole("button", { name: "刷新仓库" })).toBeEnabled();
  expect(sourceApi.listSkillRepositories).toHaveBeenCalledTimes(2);
  expect(sourceApi.scanSkillRepositories).not.toHaveBeenCalled();
});

it("rejects credential-bearing repository URLs without passing them to storage", async () => {
  const user = userEvent.setup();
  render(<SkillSourceBrowser {...sourceProps()} />);
  const dialog = await openManager(user);
  await user.click(within(dialog).getByRole("button", { name: "添加仓库" }));
  await user.type(within(dialog).getByRole("textbox", { name: "GitHub 仓库" }), "https://user:private-value@github.com/new/skills");
  await user.click(within(dialog).getByRole("button", { name: "保存仓库" }));
  expect(within(dialog).getByRole("alert")).toHaveTextContent("不含凭据");
  expect(within(dialog).getByRole("alert")).not.toHaveTextContent("private-value");
  expect(sourceApi.saveSkillRepository).not.toHaveBeenCalled();
});
