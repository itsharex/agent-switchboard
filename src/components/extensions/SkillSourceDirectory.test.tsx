import {
  candidate, deferred, directoryEntry, libraryItem, otherDirectoryEntry, searchDirectory,
  second, sourceApi, sourceProps,
} from "./skill-source-test-support";
import { expect, it } from "vitest";
import { act, fireEvent, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { SkillCandidateDto } from "../../api/client";
import type { SkillDirectoryResult } from "../../api/extensions/skill-sources";
import { SkillSourceBrowser } from "./SkillSourceBrowser";

it("queries skills.sh only on explicit submit, validates short queries, and loads more without duplicate cards", async () => {
  sourceApi.searchSkillDirectory.mockResolvedValueOnce({ items: [directoryEntry], total: 3, hasMore: true })
    .mockResolvedValueOnce({ items: [directoryEntry, otherDirectoryEntry], total: 3, hasMore: true })
    .mockResolvedValueOnce({ items: [], total: 3, hasMore: false });
  const user = userEvent.setup();
  render(<SkillSourceBrowser {...sourceProps()} />);
  await user.click(screen.getByRole("radio", { name: "skills.sh" }));
  const input = screen.getByRole("searchbox", { name: "搜索 skills.sh" });
  await user.type(input, "r");
  await user.click(screen.getByRole("button", { name: "搜索" }));
  expect(sourceApi.searchSkillDirectory).not.toHaveBeenCalled();
  expect(screen.getByRole("alert")).toHaveTextContent("至少需要 2 个字符");
  await user.type(input, "elease");
  expect(sourceApi.searchSkillDirectory).not.toHaveBeenCalled();
  await user.click(screen.getByRole("button", { name: "搜索" }));
  expect(sourceApi.searchSkillDirectory).toHaveBeenLastCalledWith("release", 0);
  const results = screen.getByRole("region", { name: "skills.sh 搜索结果" });
  expect(within(results).getByText("1,234 次目录安装")).toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "加载更多" }));
  expect(sourceApi.searchSkillDirectory).toHaveBeenLastCalledWith("release", 1);
  expect(within(results).getAllByRole("listitem")).toHaveLength(2);
  await user.click(screen.getByRole("button", { name: "加载更多" }));
  expect(sourceApi.searchSkillDirectory).toHaveBeenLastCalledWith("release", 3);
  expect(screen.queryByRole("button", { name: "加载更多" })).not.toBeInTheDocument();
});

it("resolves a directory entry before installation, locks duplicate requests, and uses the real manifest", async () => {
  const pending = deferred<SkillCandidateDto[]>();
  sourceApi.resolveDirectorySkill.mockReturnValueOnce(pending.promise);
  const callbacks = sourceProps();
  const user = userEvent.setup();
  render(<SkillSourceBrowser {...callbacks} />);
  await searchDirectory(user);
  const install = screen.getByRole("button", { name: "安装 release-notes" });
  fireEvent.click(install); fireEvent.click(install);
  expect(sourceApi.resolveDirectorySkill).toHaveBeenCalledOnce();
  expect(sourceApi.resolveDirectorySkill).toHaveBeenCalledWith(directoryEntry);
  expect(callbacks.onImport).not.toHaveBeenCalled();
  expect(screen.getByRole("button", { name: "安装 api-spec" })).toBeDisabled();
  await act(async () => pending.resolve([{ ...candidate, description: null }]));
  expect(callbacks.onImport).toHaveBeenCalledWith(candidate.digest, candidate.name, "claude");
  expect(screen.getByText("仅 Claude")).toBeInTheDocument();
  expect(screen.queryByRole("button", { name: "已安装 release-notes" })).not.toBeInTheDocument();
});

it("requires the actual candidate choice when directory resolution returns multiple Skills", async () => {
  sourceApi.searchSkillDirectory.mockResolvedValue({ items: [directoryEntry], total: 1, hasMore: false });
  sourceApi.resolveDirectorySkill.mockResolvedValue([candidate, second]);
  const callbacks = sourceProps();
  const user = userEvent.setup();
  render(<SkillSourceBrowser {...callbacks} />);
  await searchDirectory(user);
  await user.click(screen.getByRole("button", { name: "安装 release-notes" }));
  expect(callbacks.onImport).not.toHaveBeenCalled();
  expect(screen.getByRole("listitem", { name: "release-notes" })).toHaveTextContent("整理版本说明");
  await user.click(screen.getByRole("button", { name: "安装 api-spec" }));
  expect(callbacks.onImport).toHaveBeenCalledWith(second.digest, second.name, null);
});

it("shows rejected directory candidates and never imports them", async () => {
  sourceApi.resolveDirectorySkill.mockResolvedValue([{ ...candidate, diagnostics: ["清单缺少 name"] }]);
  const callbacks = sourceProps();
  const user = userEvent.setup();
  render(<SkillSourceBrowser {...callbacks} />);
  await searchDirectory(user);
  await user.click(screen.getByRole("button", { name: "安装 release-notes" }));
  expect(screen.getByRole("button", { name: "不可安装 release-notes" })).toBeDisabled();
  expect(screen.getByText("清单缺少 name")).toBeInTheDocument();
  expect(callbacks.onImport).not.toHaveBeenCalled();
});

it("keeps the result list when resolution fails and retries the same entry", async () => {
  sourceApi.resolveDirectorySkill.mockRejectedValueOnce(new Error("仓库返回 HTTP 429"));
  const callbacks = sourceProps();
  const user = userEvent.setup();
  render(<SkillSourceBrowser {...callbacks} />);
  await searchDirectory(user);
  await user.click(screen.getByRole("button", { name: "安装 release-notes" }));
  expect(screen.getByRole("alert")).toHaveTextContent("仓库返回 HTTP 429");
  expect(screen.getByRole("listitem", { name: "api-spec" })).toBeInTheDocument();
  expect(callbacks.onImport).not.toHaveBeenCalled();
  await user.click(screen.getByRole("button", { name: "安装 release-notes" }));
  expect(sourceApi.resolveDirectorySkill).toHaveBeenCalledTimes(2);
  expect(callbacks.onImport).toHaveBeenCalledOnce();
});

it("allows retry when a directory entry has no resolved candidates", async () => {
  sourceApi.resolveDirectorySkill.mockResolvedValueOnce([]);
  const callbacks = sourceProps();
  const user = userEvent.setup();
  render(<SkillSourceBrowser {...callbacks} />);
  await searchDirectory(user);
  await user.click(screen.getByRole("button", { name: "安装 release-notes" }));
  expect(screen.getByText("未找到 SKILL.md")).toBeInTheDocument();
  expect(callbacks.onImport).not.toHaveBeenCalled();
  await user.click(screen.getByRole("button", { name: "重试解析 release-notes" }));
  expect(callbacks.onImport).toHaveBeenCalledWith(candidate.digest, candidate.name, null);
});

it("marks a directory entry installed only after matching its digest against actual deployments", async () => {
  const callbacks = sourceProps();
  const user = userEvent.setup();
  const view = render(<SkillSourceBrowser {...callbacks} items={[libraryItem(true)]} />);
  await searchDirectory(user);
  expect(screen.queryByRole("button", { name: "已安装 release-notes" })).not.toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "安装 release-notes" }));
  expect(callbacks.onImport).not.toHaveBeenCalled();
  expect(screen.getByRole("button", { name: "已安装 release-notes" })).toBeDisabled();
  view.rerender(<SkillSourceBrowser {...callbacks} items={[libraryItem(true, ["codex"])]} />);
  expect(screen.getByRole("button", { name: "安装 release-notes" })).toBeEnabled();
});

it("keeps previous results on search or load-more failure and retries the same offset", async () => {
  sourceApi.searchSkillDirectory.mockResolvedValueOnce({ items: [directoryEntry], total: 2, hasMore: true })
    .mockRejectedValueOnce(new Error("目录搜索暂不可用"))
    .mockResolvedValueOnce({ items: [otherDirectoryEntry], total: 2, hasMore: false })
    .mockRejectedValueOnce(new Error("网络不可达"));
  const user = userEvent.setup();
  render(<SkillSourceBrowser {...sourceProps()} />);
  await searchDirectory(user);
  await user.click(screen.getByRole("button", { name: "加载更多" }));
  expect(screen.getByRole("alert")).toHaveTextContent("目录搜索暂不可用；仍显示上次结果");
  expect(screen.getByRole("listitem", { name: "release-notes" })).toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "加载更多" }));
  expect(sourceApi.searchSkillDirectory).toHaveBeenNthCalledWith(2, "release", 1);
  expect(sourceApi.searchSkillDirectory).toHaveBeenNthCalledWith(3, "release", 1);
  const input = screen.getByRole("searchbox", { name: "搜索 skills.sh" });
  await user.clear(input); await user.type(input, "new query");
  await user.click(screen.getByRole("button", { name: "搜索" }));
  expect(screen.getByRole("alert")).toHaveTextContent("网络不可达；仍显示上次结果");
  expect(screen.getByRole("region", { name: "skills.sh 搜索结果" })).toHaveTextContent("release · 已加载 2 / 2");
});

it("ignores an earlier query response without replacing or unlocking the newer query", async () => {
  const old = deferred<SkillDirectoryResult>();
  const current = deferred<SkillDirectoryResult>();
  sourceApi.searchSkillDirectory.mockReturnValueOnce(old.promise).mockReturnValueOnce(current.promise);
  const user = userEvent.setup();
  render(<SkillSourceBrowser {...sourceProps()} />);
  await user.click(screen.getByRole("radio", { name: "skills.sh" }));
  const input = screen.getByRole("searchbox", { name: "搜索 skills.sh" });
  await user.type(input, "old");
  fireEvent.submit(screen.getByRole("form", { name: "Skill 来源" }));
  await user.clear(input); await user.type(input, "new");
  fireEvent.submit(screen.getByRole("form", { name: "Skill 来源" }));
  await act(async () => old.resolve({ items: [directoryEntry], total: 1, hasMore: false }));
  expect(screen.getByRole("button", { name: "正在搜索…" })).toBeDisabled();
  expect(screen.queryByRole("listitem", { name: "release-notes" })).not.toBeInTheDocument();
  await act(async () => current.resolve({ items: [otherDirectoryEntry], total: 1, hasMore: false }));
  expect(screen.getByRole("listitem", { name: "api-spec" })).toBeInTheDocument();
});

it("does not append a late old page to a new query", async () => {
  const oldPage = deferred<SkillDirectoryResult>();
  sourceApi.searchSkillDirectory.mockResolvedValueOnce({ items: [directoryEntry], total: 2, hasMore: true })
    .mockReturnValueOnce(oldPage.promise)
    .mockResolvedValueOnce({ items: [otherDirectoryEntry], total: 1, hasMore: false });
  const user = userEvent.setup();
  render(<SkillSourceBrowser {...sourceProps()} />);
  await searchDirectory(user);
  await user.click(screen.getByRole("button", { name: "加载更多" }));
  const input = screen.getByRole("searchbox", { name: "搜索 skills.sh" });
  await user.clear(input); await user.type(input, "new");
  await user.click(screen.getByRole("button", { name: "搜索" }));
  await act(async () => oldPage.resolve({ items: [directoryEntry], total: 2, hasMore: false }));
  expect(screen.getByRole("region", { name: "skills.sh 搜索结果" })).toHaveTextContent("new · 已加载 1 / 1");
  expect(screen.queryByRole("listitem", { name: "release-notes" })).not.toBeInTheDocument();
});

it("does not import a late directory resolution after source changes or unmount", async () => {
  const pending = deferred<SkillCandidateDto[]>();
  sourceApi.resolveDirectorySkill.mockReturnValueOnce(pending.promise);
  const callbacks = sourceProps();
  const user = userEvent.setup();
  const view = render(<SkillSourceBrowser {...callbacks} />);
  await searchDirectory(user);
  await user.click(screen.getByRole("button", { name: "安装 release-notes" }));
  await user.click(screen.getByRole("radio", { name: "仓库目录" }));
  await act(async () => pending.resolve([candidate]));
  expect(callbacks.onImport).not.toHaveBeenCalled();
  const afterUnmount = deferred<SkillCandidateDto[]>();
  sourceApi.resolveDirectorySkill.mockReturnValueOnce(afterUnmount.promise);
  await user.click(screen.getByRole("radio", { name: "skills.sh" }));
  await user.click(screen.getByRole("button", { name: "安装 release-notes" }));
  view.unmount();
  await act(async () => afterUnmount.resolve([candidate]));
  expect(callbacks.onImport).not.toHaveBeenCalled();
});

it("honors an external write block that arrives while a directory entry is resolving", async () => {
  const pending = deferred<SkillCandidateDto[]>();
  sourceApi.resolveDirectorySkill.mockReturnValueOnce(pending.promise);
  const callbacks = sourceProps();
  const user = userEvent.setup();
  const view = render(<SkillSourceBrowser {...callbacks} />);
  await searchDirectory(user);
  await user.click(screen.getByRole("button", { name: "安装 release-notes" }));
  view.rerender(<SkillSourceBrowser {...callbacks} busy />);
  await act(async () => pending.resolve([candidate]));
  expect(callbacks.onImport).not.toHaveBeenCalled();
  expect(screen.getByRole("button", { name: "安装 release-notes" })).toBeDisabled();
});
