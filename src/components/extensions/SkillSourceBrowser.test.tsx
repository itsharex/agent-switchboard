import {
  candidate, catalogCandidate, deferred, libraryItem, otherRepository, refreshCatalog,
  repository, second, sourceApi, sourceProps,
} from "./skill-source-test-support";
import { expect, it } from "vitest";
import { act, fireEvent, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { ExtensionMutation, SkillCandidateDto } from "../../api/client";
import type { SkillCatalogResult } from "../../api/extensions/skill-sources";
import { SkillSourceBrowser } from "./SkillSourceBrowser";

it("defaults to the saved repository catalog, reads only local settings on mount, and installs explicitly", async () => {
  const callbacks = sourceProps();
  const user = userEvent.setup();
  render(<SkillSourceBrowser {...callbacks} />);
  expect(screen.getByRole("radio", { name: "仓库目录" })).toBeChecked();
  expect(sourceApi.scanSkillRepositories).not.toHaveBeenCalled();
  expect(sourceApi.searchSkillDirectory).not.toHaveBeenCalled();
  expect(sourceApi.resolveDirectorySkill).not.toHaveBeenCalled();
  expect(sourceApi.scanSkillZip).not.toHaveBeenCalled();
  expect(callbacks.onScanLocal).not.toHaveBeenCalled();
  const results = await refreshCatalog(user);
  expect(sourceApi.listSkillRepositories).toHaveBeenCalledOnce();
  expect(within(results).getAllByRole("listitem")).toHaveLength(2);
  expect(within(results).getByText("整理版本说明")).toBeInTheDocument();
  expect(within(results).getByRole("link", { name: "查看 release-notes 来源" })).toHaveAttribute(
    "href", "https://github.com/example/skills/blob/main/skills/release-notes/SKILL.md");
  await user.click(within(results).getByRole("button", { name: "安装 release-notes" }));
  expect(callbacks.onImport).toHaveBeenCalledWith(candidate.digest, candidate.name, null);
  expect(within(results).queryByRole("button", { name: "已安装 release-notes" })).not.toBeInTheDocument();
});

it("filters repositories, descriptions and live installation state without a network call", async () => {
  sourceApi.listSkillRepositories.mockResolvedValue([repository, otherRepository]);
  sourceApi.scanSkillRepositories.mockResolvedValue({
    candidates: [catalogCandidate(candidate), catalogCandidate(second, otherRepository)], failures: [],
  });
  const callbacks = sourceProps();
  callbacks.items = [libraryItem(true)];
  const user = userEvent.setup();
  render(<SkillSourceBrowser {...callbacks} />);
  const results = await refreshCatalog(user);
  expect(within(results).getByRole("button", { name: "已安装 release-notes" })).toBeDisabled();
  await user.click(screen.getByRole("combobox", { name: "Skill 安装状态" }));
  await user.click(screen.getByRole("option", { name: "待安装" }));
  expect(within(results).queryByRole("listitem", { name: "release-notes" })).not.toBeInTheDocument();
  await user.click(screen.getByRole("combobox", { name: "Skill 来源仓库" }));
  await user.click(screen.getByRole("option", { name: "another/tools" }));
  const search = screen.getByRole("searchbox", { name: "筛选发现的 Skills" });
  await user.type(search, "API 文档");
  expect(within(results).getByRole("listitem", { name: "api-spec" })).toBeInTheDocument();
  await user.clear(search);
  await user.type(search, "another/tools");
  expect(within(results).getByRole("listitem", { name: "api-spec" })).toBeInTheDocument();
  await user.clear(search);
  await user.type(search, "no match");
  await user.click(screen.getByRole("button", { name: "清除筛选" }));
  expect(within(results).getAllByRole("listitem")).toHaveLength(2);
  expect(sourceApi.scanSkillRepositories).toHaveBeenCalledOnce();
});

it("does not label library-only, partial, drifted or differently pinned deployments as installed", async () => {
  const callbacks = sourceProps();
  const user = userEvent.setup();
  const view = render(<SkillSourceBrowser {...callbacks} items={[libraryItem(false)]} />);
  await refreshCatalog(user);
  expect(screen.getByRole("button", { name: "安装 release-notes" })).toBeEnabled();
  view.rerender(<SkillSourceBrowser {...callbacks} items={[libraryItem(true, ["codex"])]} />);
  expect(screen.getByRole("button", { name: "安装 release-notes" })).toBeEnabled();
  const drifted = libraryItem(true);
  drifted.bindings[0].fileState = "externalChange";
  view.rerender(<SkillSourceBrowser {...callbacks} items={[drifted]} />);
  expect(screen.getByRole("button", { name: "安装 release-notes" })).toBeEnabled();
  const pinned = libraryItem(true);
  pinned.bindings[0].lockedDigest = second.digest;
  view.rerender(<SkillSourceBrowser {...callbacks} items={[pinned]} />);
  expect(screen.getByRole("button", { name: "安装 release-notes" })).toBeEnabled();
  view.rerender(<SkillSourceBrowser {...callbacks} items={[libraryItem(true)]} />);
  expect(screen.getByRole("button", { name: "已安装 release-notes" })).toBeDisabled();
});

it("locks imports across candidates and source controls, then permits retry after failure", async () => {
  const callbacks = sourceProps();
  const pending = deferred<ExtensionMutation | null>();
  callbacks.onImport.mockReturnValueOnce(pending.promise);
  const user = userEvent.setup();
  render(<SkillSourceBrowser {...callbacks} />);
  await refreshCatalog(user);
  const install = screen.getByRole("button", { name: "安装 release-notes" });
  fireEvent.click(install); fireEvent.click(install);
  expect(callbacks.onImport).toHaveBeenCalledOnce();
  expect(screen.getByRole("button", { name: "正在安装 release-notes" })).toBeDisabled();
  expect(screen.getByRole("button", { name: "安装 api-spec" })).toBeDisabled();
  expect(screen.getByRole("button", { name: "管理仓库" })).toBeDisabled();
  expect(screen.getByRole("radio", { name: "skills.sh" })).toBeDisabled();
  await act(async () => pending.reject(new Error("安装目录暂不可写")));
  expect(screen.getByRole("alert")).toHaveTextContent("安装目录暂不可写");
  await user.click(screen.getByRole("button", { name: "安装 release-notes" }));
  expect(callbacks.onImport).toHaveBeenCalledTimes(2);
});

it("rejects malformed manifests and scopes description-less Skills to Claude", async () => {
  sourceApi.scanSkillRepositories.mockResolvedValue({ candidates: [
    catalogCandidate({ ...candidate, description: null }), catalogCandidate({ ...second, diagnostics: ["frontmatter 未闭合"] }),
  ], failures: [] });
  const callbacks = sourceProps();
  const user = userEvent.setup();
  render(<SkillSourceBrowser {...callbacks} />);
  await refreshCatalog(user);
  expect(screen.getByText("frontmatter 未闭合")).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "不可安装 api-spec" })).toBeDisabled();
  await user.click(screen.getByRole("button", { name: "安装 release-notes" }));
  expect(callbacks.onImport).toHaveBeenCalledWith(candidate.digest, candidate.name, "claude");
});

it("preserves successful catalog results and actionable errors when refresh fails", async () => {
  sourceApi.scanSkillRepositories.mockResolvedValueOnce({ candidates: [catalogCandidate(candidate)], failures: [] })
    .mockRejectedValueOnce({ message: "GitHub 返回 HTTP 429" });
  const user = userEvent.setup();
  render(<SkillSourceBrowser {...sourceProps()} />);
  await refreshCatalog(user);
  await user.click(screen.getByRole("button", { name: "刷新仓库" }));
  expect(screen.getByRole("alert")).toHaveTextContent("GitHub 返回 HTTP 429；仍显示上次结果");
  expect(screen.getByRole("listitem", { name: "release-notes" })).toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "刷新仓库" }));
  expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  expect(screen.getByRole("listitem", { name: "api-spec" })).toBeInTheDocument();
});

it("keeps a failed repository's cached candidates alongside freshly scanned repositories", async () => {
  sourceApi.listSkillRepositories.mockResolvedValue([repository, otherRepository]);
  sourceApi.scanSkillRepositories.mockResolvedValueOnce({ candidates: [catalogCandidate(candidate)], failures: [] })
    .mockResolvedValueOnce({ candidates: [catalogCandidate(second, otherRepository)],
      failures: [{ repositoryId: repository.id, repo: repository.repo, message: "HTTP 503" }] });
  const user = userEvent.setup();
  render(<SkillSourceBrowser {...sourceProps()} />);
  await refreshCatalog(user);
  await user.click(screen.getByRole("button", { name: "刷新仓库" }));
  expect(screen.getByRole("alert")).toHaveTextContent("example/skills：HTTP 503；仍显示上次结果");
  expect(screen.getByRole("listitem", { name: "release-notes" })).toBeInTheDocument();
  expect(screen.getByRole("listitem", { name: "api-spec" })).toBeInTheDocument();
  sourceApi.scanSkillRepositories.mockResolvedValueOnce({ candidates: [], failures: [] });
  await user.click(screen.getByRole("button", { name: "重试刷新" }));
  expect(screen.queryByRole("listitem", { name: "release-notes" })).not.toBeInTheDocument();
  expect(screen.queryByRole("alert")).not.toBeInTheDocument();
});

it("ignores an old catalog response without unlocking a newer ZIP scan", async () => {
  const old = deferred<SkillCatalogResult>();
  const current = deferred<SkillCandidateDto[]>();
  sourceApi.scanSkillRepositories.mockReturnValueOnce(old.promise);
  sourceApi.scanSkillZip.mockReturnValueOnce(current.promise);
  const user = userEvent.setup();
  render(<SkillSourceBrowser {...sourceProps()} />);
  await user.click(await screen.findByRole("button", { name: "刷新仓库" }));
  fireEvent.submit(screen.getByRole("form", { name: "Skill 来源" }));
  expect(sourceApi.scanSkillRepositories).toHaveBeenCalledOnce();
  await user.click(screen.getByRole("radio", { name: "ZIP 文件" }));
  await user.type(screen.getByRole("textbox", { name: "ZIP 文件路径" }), "D:\\new.zip");
  await user.click(screen.getByRole("button", { name: "扫描 ZIP" }));
  await act(async () => old.resolve({ candidates: [catalogCandidate(candidate)], failures: [] }));
  expect(screen.getByRole("button", { name: "正在扫描…" })).toBeDisabled();
  expect(screen.queryByRole("region", { name: "Skill 来源候选" })).not.toBeInTheDocument();
  await act(async () => current.resolve([second]));
  expect(screen.getByRole("listitem", { name: "api-spec" })).toBeInTheDocument();
  expect(screen.queryByRole("listitem", { name: "release-notes" })).not.toBeInTheDocument();
});
