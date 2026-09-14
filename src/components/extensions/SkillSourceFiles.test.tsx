import { candidate, deferred, second, sourceApi, sourceProps } from "./skill-source-test-support";
import { expect, it } from "vitest";
import { act, fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { SkillCandidateDto } from "../../api/client";
import { SkillSourceBrowser } from "./SkillSourceBrowser";

it("previews a manually supplied ZIP before installation and displays candidate diagnostics", async () => {
  sourceApi.scanSkillZip.mockResolvedValue([
    candidate, { ...second, diagnostics: ["无效的 SKILL.md frontmatter"] },
  ]);
  const callbacks = sourceProps();
  const user = userEvent.setup();
  render(<SkillSourceBrowser {...callbacks} />);
  await user.click(screen.getByRole("radio", { name: "ZIP 文件" }));
  await user.type(screen.getByRole("textbox", { name: "ZIP 文件路径" }), "D:\\imports\\skills.zip");
  expect(sourceApi.scanSkillZip).not.toHaveBeenCalled();
  await user.click(screen.getByRole("button", { name: "扫描 ZIP" }));
  expect(sourceApi.scanSkillZip).toHaveBeenCalledWith("D:\\imports\\skills.zip");
  expect(callbacks.onImport).not.toHaveBeenCalled();
  expect(screen.getByRole("listitem", { name: "release-notes" })).toHaveTextContent("3 个文件");
  expect(screen.getByRole("button", { name: "不可安装 api-spec" })).toBeDisabled();
  await user.click(screen.getByRole("button", { name: "安装 release-notes" }));
  expect(callbacks.onImport).toHaveBeenCalledWith(candidate.digest, candidate.name, null);
});

it("scans a selected ZIP and preserves the manual path and preview when selection is cancelled", async () => {
  const callbacks = sourceProps();
  callbacks.onPickZip.mockResolvedValueOnce(null).mockResolvedValueOnce("D:\\picked.zip");
  const user = userEvent.setup();
  render(<SkillSourceBrowser {...callbacks} />);
  await user.click(screen.getByRole("radio", { name: "ZIP 文件" }));
  const path = screen.getByRole("textbox", { name: "ZIP 文件路径" });
  await user.type(path, "D:\\manual.zip");
  await user.click(screen.getByRole("button", { name: "扫描 ZIP" }));
  const filter = screen.getByRole("searchbox", { name: "筛选发现的 Skills" });
  await user.type(filter, "release");
  await user.click(screen.getByRole("button", { name: "选择 ZIP" }));
  expect(path).toHaveValue("D:\\manual.zip");
  expect(filter).toHaveValue("release");
  expect(sourceApi.scanSkillZip).toHaveBeenCalledOnce();
  await user.click(screen.getByRole("button", { name: "选择 ZIP" }));
  expect(sourceApi.scanSkillZip).toHaveBeenLastCalledWith("D:\\picked.zip");
  expect(path).toHaveValue("D:\\picked.zip");
  expect(screen.getByRole("searchbox", { name: "筛选发现的 Skills" })).toHaveValue("");
});

it("does not scan a late ZIP selection after switching sources", async () => {
  const callbacks = sourceProps();
  const selected = deferred<string>();
  callbacks.onPickZip.mockReturnValueOnce(selected.promise);
  const user = userEvent.setup();
  render(<SkillSourceBrowser {...callbacks} />);
  await user.click(screen.getByRole("radio", { name: "ZIP 文件" }));
  await user.click(screen.getByRole("button", { name: "选择 ZIP" }));
  await user.click(screen.getByRole("radio", { name: "本地目录" }));
  await act(async () => selected.resolve("D:\\obsolete.zip"));
  expect(sourceApi.scanSkillZip).not.toHaveBeenCalled();
  expect(screen.getByRole("textbox", { name: "本地来源目录" })).toHaveValue("");
});

it("preserves ZIP candidates on scan errors and retries the same path", async () => {
  sourceApi.scanSkillZip.mockResolvedValueOnce([candidate]).mockRejectedValueOnce(new Error("ZIP 文件无法读取"));
  const user = userEvent.setup();
  render(<SkillSourceBrowser {...sourceProps()} />);
  await user.click(screen.getByRole("radio", { name: "ZIP 文件" }));
  await user.type(screen.getByRole("textbox", { name: "ZIP 文件路径" }), "D:\\skills.zip");
  await user.click(screen.getByRole("button", { name: "扫描 ZIP" }));
  await user.click(screen.getByRole("button", { name: "扫描 ZIP" }));
  expect(screen.getByRole("alert")).toHaveTextContent("ZIP 文件无法读取；仍显示上次结果");
  expect(screen.getByRole("listitem", { name: "release-notes" })).toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "扫描 ZIP" }));
  expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  expect(screen.getByRole("listitem", { name: "api-spec" })).toBeInTheDocument();
});

it("retains a picked path for retry after scanning fails without discarding the earlier preview", async () => {
  sourceApi.scanSkillZip.mockResolvedValueOnce([candidate]).mockRejectedValueOnce(new Error("ZIP 暂不可读"));
  const callbacks = sourceProps();
  callbacks.onPickZip.mockResolvedValue("D:\\retry.zip");
  const user = userEvent.setup();
  render(<SkillSourceBrowser {...callbacks} />);
  await user.click(screen.getByRole("radio", { name: "ZIP 文件" }));
  await user.type(screen.getByRole("textbox", { name: "ZIP 文件路径" }), "D:\\old.zip");
  await user.click(screen.getByRole("button", { name: "扫描 ZIP" }));
  await user.click(screen.getByRole("button", { name: "选择 ZIP" }));
  expect(screen.getByRole("textbox", { name: "ZIP 文件路径" })).toHaveValue("D:\\retry.zip");
  expect(screen.getByRole("listitem", { name: "release-notes" })).toHaveTextContent("D:\\old.zip");
  expect(screen.getByRole("alert")).toHaveTextContent("ZIP 暂不可读；仍显示上次结果");
  await user.click(screen.getByRole("button", { name: "扫描 ZIP" }));
  expect(sourceApi.scanSkillZip).toHaveBeenLastCalledWith("D:\\retry.zip");
  expect(screen.queryByRole("alert")).not.toBeInTheDocument();
});

it("preserves local directory scanning and reports null results without deleting the last success", async () => {
  const callbacks = sourceProps();
  callbacks.onScanLocal.mockResolvedValueOnce([candidate]).mockResolvedValueOnce(null);
  const user = userEvent.setup();
  render(<SkillSourceBrowser {...callbacks} />);
  await user.click(screen.getByRole("radio", { name: "本地目录" }));
  await user.type(screen.getByRole("textbox", { name: "本地来源目录" }), "D:\\skills");
  expect(callbacks.onScanLocal).not.toHaveBeenCalled();
  await user.click(screen.getByRole("button", { name: "扫描来源" }));
  expect(callbacks.onScanLocal).toHaveBeenCalledWith("D:\\skills");
  await user.click(screen.getByRole("button", { name: "扫描来源" }));
  expect(screen.getByRole("alert")).toHaveTextContent("来源读取失败；详细原因见操作通知；仍显示上次结果");
  expect(screen.getByRole("listitem", { name: "release-notes" })).toBeInTheDocument();
  expect(sourceApi.scanSkillRepositories).not.toHaveBeenCalled();
});

it("does not describe a previous ZIP preview as a preserved local-directory result", async () => {
  const callbacks = sourceProps();
  callbacks.onPickDirectory.mockResolvedValue("D:\\unreadable");
  callbacks.onScanLocal.mockRejectedValue(new Error("目录无法读取"));
  const user = userEvent.setup();
  render(<SkillSourceBrowser {...callbacks} />);
  await user.click(screen.getByRole("radio", { name: "ZIP 文件" }));
  await user.type(screen.getByRole("textbox", { name: "ZIP 文件路径" }), "D:\\skills.zip");
  await user.click(screen.getByRole("button", { name: "扫描 ZIP" }));
  await user.click(screen.getByRole("radio", { name: "本地目录" }));
  await user.click(screen.getByRole("button", { name: "浏览目录" }));
  expect(screen.getByRole("alert")).toHaveTextContent("目录无法读取");
  expect(screen.getByRole("alert")).not.toHaveTextContent("仍显示上次结果");
  expect(screen.queryByRole("region", { name: "Skill 来源候选" })).not.toBeInTheDocument();
});

it("keeps a typed local path on picker cancellation and scans the picked path on acceptance", async () => {
  const callbacks = sourceProps();
  callbacks.onPickDirectory.mockResolvedValueOnce(null).mockResolvedValueOnce("D:\\picked-skills");
  const user = userEvent.setup();
  render(<SkillSourceBrowser {...callbacks} />);
  await user.click(screen.getByRole("radio", { name: "本地目录" }));
  await user.type(screen.getByRole("textbox", { name: "本地来源目录" }), "D:\\typed-skills");
  await user.click(screen.getByRole("button", { name: "浏览目录" }));
  expect(callbacks.onScanLocal).not.toHaveBeenCalled();
  expect(screen.getByRole("textbox", { name: "本地来源目录" })).toHaveValue("D:\\typed-skills");
  await user.click(screen.getByRole("button", { name: "浏览目录" }));
  expect(callbacks.onScanLocal).toHaveBeenCalledWith("D:\\picked-skills");
  expect(screen.getByRole("textbox", { name: "本地来源目录" })).toHaveValue("D:\\picked-skills");
});

it("ignores obsolete local scans, including their errors, while a newer path is loading", async () => {
  const callbacks = sourceProps();
  const old = deferred<SkillCandidateDto[]>();
  const current = deferred<SkillCandidateDto[]>();
  callbacks.onScanLocal.mockReturnValueOnce(old.promise).mockReturnValueOnce(current.promise);
  const user = userEvent.setup();
  render(<SkillSourceBrowser {...callbacks} />);
  await user.click(screen.getByRole("radio", { name: "本地目录" }));
  const path = screen.getByRole("textbox", { name: "本地来源目录" });
  await user.type(path, "D:\\old");
  fireEvent.submit(screen.getByRole("form", { name: "Skill 来源" }));
  fireEvent.submit(screen.getByRole("form", { name: "Skill 来源" }));
  expect(callbacks.onScanLocal).toHaveBeenCalledOnce();
  await user.clear(path);
  await user.type(path, "D:\\new");
  await user.click(screen.getByRole("button", { name: "扫描来源" }));
  await act(async () => old.reject(new Error("过期错误")));
  expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  expect(screen.getByRole("button", { name: "正在扫描…" })).toBeDisabled();
  await act(async () => current.resolve([second]));
  expect(screen.getByRole("listitem", { name: "api-spec" })).toBeInTheDocument();
  expect(screen.queryByRole("listitem", { name: "release-notes" })).not.toBeInTheDocument();
});

it("does not scan a late folder selection after switching to the repository catalog", async () => {
  const callbacks = sourceProps();
  const selected = deferred<string>();
  callbacks.onPickDirectory.mockReturnValueOnce(selected.promise);
  const user = userEvent.setup();
  render(<SkillSourceBrowser {...callbacks} />);
  await user.click(screen.getByRole("radio", { name: "本地目录" }));
  await user.click(screen.getByRole("button", { name: "浏览目录" }));
  await user.click(screen.getByRole("radio", { name: "仓库目录" }));
  await act(async () => selected.resolve("D:\\obsolete"));
  expect(callbacks.onScanLocal).not.toHaveBeenCalled();
  expect(screen.queryByRole("region", { name: "Skill 来源候选" })).not.toBeInTheDocument();
});
