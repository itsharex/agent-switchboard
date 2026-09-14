import assert from "node:assert/strict";
import { readFile, readdir } from "node:fs/promises";
import path from "node:path";
import { clickButton, fill, navigate, openDiagnosticsSection, readSandbox, selectOption, visibleElement, xpathText } from "./ui.mjs";

const protocolLabels = {
  responses: "Responses (/responses)",
  chatCompletions: "Chat Completions (/chat/completions)",
  anthropicMessages: "Anthropic Messages (/v1/messages)",
};

export function ensureSandboxPath(file) {
  const relative = path.relative(readSandbox().root, path.resolve(file));
  assert(relative && !relative.startsWith("..") && !path.isAbsolute(relative),
    `E2E file must be inside the isolated run: ${file}`);
  return file;
}

export function clientFile(app) {
  const sandbox = readSandbox();
  assert(app === "codex" || app === "claude");
  return ensureSandboxPath(path.join(sandbox[app], app === "codex" ? "config.toml" : "settings.json"));
}

export async function readOptional(file) {
  ensureSandboxPath(file);
  try {
    return await readFile(file, "utf8");
  } catch (error) {
    if (error.code === "ENOENT") return null;
    throw error;
  }
}

export async function jsonFile(file) {
  const content = await readOptional(file);
  assert.notEqual(content, null, `Expected persisted JSON: ${file}`);
  return JSON.parse(content);
}

export async function providerFiles(app) {
  const directory = ensureSandboxPath(path.join(readSandbox().state, "configuration", "providers", app));
  let files;
  try {
    files = await readdir(directory);
  } catch (error) {
    if (error.code === "ENOENT") return [];
    throw error;
  }
  const records = await Promise.all(files.filter((file) => file.endsWith(".json")).map(async (file) => {
    const filePath = path.join(directory, file);
    return { ...await jsonFile(filePath), filePath };
  }));
  return records.sort((left, right) => left.position - right.position || left.id.localeCompare(right.id));
}

export async function backupRecords(directory = path.join(readSandbox().state, "backups")) {
  ensureSandboxPath(directory);
  let files;
  try {
    files = await readdir(directory);
  } catch (error) {
    if (error.code === "ENOENT") return [];
    throw error;
  }
  const records = await Promise.all(files.filter((file) => file.endsWith(".meta.json"))
    .map((file) => jsonFile(path.join(directory, file))));
  for (const record of records) {
    ensureSandboxPath(record.targetPath);
    ensureSandboxPath(record.backupPath);
  }
  return records.sort((left, right) => right.createdAt.localeCompare(left.createdAt));
}

export async function waitForFile(file, predicate, message) {
  await browser.waitUntil(async () => predicate(await readOptional(file)), {
    timeout: 15000,
    interval: 150,
    timeoutMsg: message ?? `Expected file state did not appear: ${file}`,
  });
}

export async function selectClient(app, groupLabel) {
  assert(app === "codex" || app === "claude");
  assert(["供应商客户端", "客户端偏好客户端", "全局指令客户端", "导入客户端"].includes(groupLabel));
  const name = app === "codex" ? "Codex" : "Claude";
  const input = await radio(groupLabel, name);
  await expect(input).toBeSelected();
}

export async function radio(groupLabel, optionText) {
  const group = await $(`[role="radiogroup"][aria-label=${JSON.stringify(groupLabel)}]`);
  const option = await group.$(`.//label[normalize-space(.)=${xpathText(optionText)}]`);
  const input = await option.$("input");
  await input.waitForEnabled();
  await option.scrollIntoView();
  await option.click();
  return input;
}

export async function refreshConfiguration() {
  await openDiagnosticsSection("配置与环境");
  const panel = await visibleElement('section[aria-label="配置状态"]');
  const button = await clickButton("刷新状态", panel);
  await browser.waitUntil(async () => button.isEnabled(), {
    timeout: 15000, timeoutMsg: "The real desktop configuration status did not finish refreshing",
  });
}

export async function openProviderImport(source) {
  assert(["本机配置", "CC Switch"].includes(source), `Unknown provider import source: ${source}`);
  await navigate("供应商");
  await clickButton("导入", await visibleElement('section[aria-label="供应商工作区"]'));
  const sources = await visibleElement('[role="group"][aria-label="导入来源"]');
  const button = await clickButton(source, sources);
  await expect(button).toHaveAttribute("aria-pressed", "true");
}

export async function clientSnapshot(app) {
  const config = await readOptional(clientFile(app));
  const auth = app === "codex" ? await readOptional(path.join(readSandbox().codex, "auth.json")) : undefined;
  const history = await readOptional(path.join(readSandbox().state, "configuration", "history", `${app}.json`));
  return { config, auth, history };
}

export async function providerSnapshot(app) {
  const records = await providerFiles(app);
  return Object.fromEntries(await Promise.all(records.map(async (record) => [record.id, await readOptional(record.filePath)])));
}

export async function providerRow(name) {
  const row = await $(`//li[.//span[@class="asb-row-name" and normalize-space(.)=${xpathText(name)}]]`);
  await row.waitForDisplayed();
  await row.scrollIntoView({ block: "center" });
  await row.moveTo();
  return row;
}

export async function setResponsesOptions({ requestMode = "standard", supportsWebsockets = false } = {}) {
  assert(["standard", "minimal"].includes(requestMode), "Unknown Responses request mode");
  assert.equal(typeof supportsWebsockets, "boolean");
  assert(!(requestMode === "minimal" && supportsWebsockets), "Minimal requests use HTTP/SSE only");
  const disclosure = await $('//details[summary/span[normalize-space(.)="Responses 能力"]]');
  if (await disclosure.getAttribute("open") === null) await disclosure.$('summary').click();
  await selectOption("请求模式", requestMode === "minimal" ? "最小请求" : "标准请求");
  const label = await $('//label[span[normalize-space(.)="支持 Responses WebSocket"]]');
  const checkbox = await label.$('input[type="checkbox"]');
  if (await checkbox.isSelected() !== supportsWebsockets) {
    await checkbox.waitForEnabled();
    await label.scrollIntoView({ block: "center" });
    await label.click();
  }
  assert.equal(await checkbox.isSelected(), supportsWebsockets);
  if (requestMode === "minimal") await expect(checkbox).toBeDisabled();
}

export async function openProviderDraft({ app = "codex", name, protocol, baseUrl, apiKey, model, maxOutputTokens, responsesOptions }) {
  const chosenProtocol = protocol ?? (app === "codex" ? "responses" : "anthropicMessages");
  assert(protocolLabels[chosenProtocol], "Unknown provider protocol");
  if (chosenProtocol !== "responses") assert(responsesOptions == null, "Only Responses profiles own Responses options");
  await navigate("供应商");
  await selectClient(app, "供应商客户端");
  await clickButton("新建供应商");
  await fill("名称", name);
  await selectOption("API 格式", protocolLabels[chosenProtocol]);
  if (chosenProtocol === "responses") await setResponsesOptions(responsesOptions ?? undefined);
  await fill("服务地址", baseUrl);
  await fill("API 密钥", apiKey);
  if (model !== undefined) await fill("主模型", model);
  if (maxOutputTokens !== undefined) await fill("最大输出 Token", String(maxOutputTokens));
}

export async function createProvider(draft) {
  await openProviderDraft(draft);
  await clickButton("保存供应商");
  await providerRow(draft.name);
  const profile = (await providerFiles(draft.app ?? "codex")).find((record) => record.name === draft.name);
  assert(profile, `UI save did not create a provider file for ${draft.name}`);
  const expectedOptions = profile.upstreamProtocol === "responses"
    ? (draft.responsesOptions ?? { requestMode: "standard", supportsWebsockets: false }) : null;
  assert.deepEqual(profile.responsesOptions, expectedOptions, "Saved Responses capabilities must match the visible controls");
  assert.equal(profile.baseUrl, draft.baseUrl, "Saving must preserve the explicitly supplied API root");
  return profile;
}

export async function previewProvider(name) {
  const row = await providerRow(name);
  const button = await clickButton(`预览 ${name} 变更`, row);
  const preview = await row.$('section[aria-label="变更预览"]');
  await preview.waitForDisplayed();
  await expect(button).toHaveAttribute("aria-expanded", "true");
  await expect(preview.$('button=确认切换')).not.toExist();
  await expect(preview.$('button=取消')).not.toExist();
  await expect($('[role="dialog"][aria-label="确认切换"]')).not.toExist();
  return preview;
}

export async function activateProvider(name) {
  await clickButton(`启用 ${name}`, await providerRow(name));
  const sheet = await $('[role="dialog"][aria-label="确认切换"]');
  await sheet.waitForDisplayed();
  assert.equal((await $$('[role="dialog"][aria-label="确认切换"]')).length, 1,
    "Activating a provider must open exactly one switch confirmation");
  return sheet;
}

export async function confirmProviderSwitch() {
  const sheet = await visibleElement('[role="dialog"][aria-label="确认切换"]');
  await clickButton("确认切换", sheet);
  await sheet.waitForDisplayed({ reverse: true });
}

export async function switchProvider(name) {
  await activateProvider(name);
  await confirmProviderSwitch();
  await browser.waitUntil(async () => (await (await providerRow(name)).getAttribute("class")).includes("is-live"), {
    timeout: 20000, interval: 200, timeoutMsg: `The real client file did not activate ${name}`,
  });
}

export async function requestDelete(name) {
  await clickButton(`删除 ${name}`);
  const sheet = await $('[role="dialog"][aria-label="删除供应商"]');
  await sheet.waitForDisplayed();
  return sheet;
}

export async function reorderOneUp(name, cancel = false) {
  const grip = await $(`button[aria-label=${JSON.stringify(`拖动调整 ${name} 的顺序`)}]`);
  await grip.scrollIntoView();
  await grip.click();
  await browser.keys(" ");
  await browser.keys("ArrowUp");
  await browser.keys(cancel ? "Escape" : " ");
}
