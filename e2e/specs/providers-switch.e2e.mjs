import assert from "node:assert/strict";
import { randomUUID } from "node:crypto";
import { mkdir, writeFile } from "node:fs/promises";
import path from "node:path";
import { clickButton, fill, navigate, readSandbox, selectOption, waitForText } from "../support/ui.mjs";
import {
  backupRecords, clientFile, clientSnapshot, confirmProviderSwitch, createProvider,
  ensureSandboxPath, jsonFile, openProviderDraft, previewProvider, providerFiles,
  providerRow, providerSnapshot, readOptional, refreshConfiguration, reorderOneUp,
  requestDelete, selectClient, switchProvider, waitForFile,
} from "../support/provider-ui.mjs";

const officialName = "E2E existing Codex official";

function draft(name, app = "codex") {
  assert(process.env.ASB_E2E_UPSTREAM_URL, "The desktop runner must start an isolated upstream fixture");
  return {
    app, name, protocol: app === "codex" ? "responses" : "anthropicMessages",
    baseUrl: `${process.env.ASB_E2E_UPSTREAM_URL}${app === "codex" ? "/v1" : ""}`, apiKey: "e2e-provider-key",
    model: `e2e-${app}-${name.toLowerCase().replaceAll(" ", "-")}`,
  };
}

async function editProvider(name) {
  await navigate("供应商");
  await providerRow(name);
  await clickButton(`编辑 ${name}`);
  await $('form[aria-label="编辑供应商"]').waitForDisplayed();
}

async function seedExistingOfficialAndHostFields() {
  const parameterSource = await createProvider(draft("E2E parameter catalog source"));
  const sandbox = readSandbox();
  const directory = ensureSandboxPath(path.join(sandbox.state, "configuration", "providers", "codex"));
  const id = randomUUID();
  await mkdir(directory, { recursive: true });
  await writeFile(path.join(directory, `${id}.json`), JSON.stringify({
    id, name: officialName, position: 50, routeMode: "official", apiKey: "",
    upstreamProtocol: null, responsesOptions: null, maxOutputTokens: null, baseUrl: null, model: null, websiteUrl: null,
    parameters: parameterSource.parameters,
  }, null, 2));
  const codex = await readOptional(clientFile("codex")) ?? "";
  await writeFile(clientFile("codex"), `host_owned_e2e = "keep-codex"\n${codex}`);
  const authPath = ensureSandboxPath(path.join(sandbox.codex, "auth.json"));
  const auth = JSON.parse(await readOptional(authPath) ?? "{}");
  await writeFile(authPath, JSON.stringify({ ...auth, e2eHostSentinel: "keep-auth" }, null, 2));
  const claude = JSON.parse(await readOptional(clientFile("claude")) ?? "{}");
  await writeFile(clientFile("claude"), JSON.stringify({
    ...claude, e2eHostSentinel: "keep-claude", env: { ...claude.env, E2E_HOST_ENV: "keep-env" },
  }, null, 2));
  await refreshConfiguration();
}

async function responsesControlsAndProjection() {
  const before = await clientSnapshot("codex");
  const filesBefore = await providerSnapshot("codex");
  await openProviderDraft({ ...draft("E2E discarded Responses options"),
    responsesOptions: { requestMode: "standard", supportsWebsockets: true } });
  const checkbox = await $('//label[span[normalize-space(.)="支持 Responses WebSocket"]]/input');
  await expect(checkbox).toBeSelected();
  await selectOption("请求模式", "最小请求");
  await expect(checkbox).not.toBeSelected();
  await expect(checkbox).toBeDisabled();
  await waitForText("最小请求固定使用 HTTP/SSE");
  await selectOption("API 格式", "Chat Completions (/chat/completions)");
  await expect($('[role="combobox"][aria-label="请求模式"]')).not.toExist();
  await selectOption("API 格式", "Responses (/responses)");
  await expect($('[role="combobox"][aria-label="请求模式"]')).toHaveText("标准请求");
  await expect($('//label[span[normalize-space(.)="支持 Responses WebSocket"]]/input')).not.toBeSelected();
  await clickButton("取消", await $('form[aria-label="新建供应商"]'));
  assert.deepEqual(await providerSnapshot("codex"), filesBefore);
  assert.deepEqual(await clientSnapshot("codex"), before);
  const profile = await createProvider({ ...draft("E2E declared WebSocket support"),
    responsesOptions: { requestMode: "standard", supportsWebsockets: true } });
  await switchProvider(profile.name);
  const config = await readOptional(clientFile("codex"));
  assert.match(config, /supports_websockets\s*=\s*true/);
  assert(config.includes(profile.baseUrl), "A standard native Responses route must use its declared API root directly");
}

async function explicitApiRootAndEndpointRejection() {
  const baseUrl = `${process.env.ASB_E2E_UPSTREAM_URL}/openai`;
  const before = await clientSnapshot("codex");
  const filesBefore = await providerSnapshot("codex");
  const requestFile = ensureSandboxPath(path.join(readSandbox().artifacts, "upstream-requests.jsonl"));
  await openProviderDraft({ ...draft("E2E explicit API root"), baseUrl });
  await clickButton("获取模型");
  await waitForFile(requestFile, (text) => text?.split(/\r?\n/).filter(Boolean)
    .some((line) => JSON.parse(line).url === "/openai/models"));
  const requests = (await readOptional(requestFile)).split(/\r?\n/).filter(Boolean).map(JSON.parse);
  const request = requests.find((entry) => entry.url === "/openai/models");
  assert.equal(request.headers.authorization, "Bearer e2e-provider-key");
  await fill("服务地址", `${baseUrl}/responses`);
  await clickButton("保存供应商");
  await waitForText("不要包含完整请求端点 /responses");
  assert.deepEqual(await providerSnapshot("codex"), filesBefore);
  assert.deepEqual(await clientSnapshot("codex"), before);
  await fill("服务地址", baseUrl);
  await clickButton("保存供应商");
  await providerRow("E2E explicit API root");
  const provider = (await providerFiles("codex")).find((file) => file.name === "E2E explicit API root");
  assert.equal(provider.baseUrl, baseUrl);
  assert.deepEqual(await clientSnapshot("codex"), before);
}

const providerCases = [
  ["cancelled new/edit drafts leave client and provider files unchanged; saved inactive edits only update the provider file", async () => {
    const before = await clientSnapshot("codex");
    const storedBefore = await providerSnapshot("codex");
    await openProviderDraft(draft("E2E discarded provider"));
    await clickButton("取消", await $('form[aria-label="新建供应商"]'));
    assert.deepEqual(await providerSnapshot("codex"), storedBefore);
    assert.deepEqual(await clientSnapshot("codex"), before);

    const profile = await createProvider(draft("E2E inactive editing"));
    assert.equal(profile.upstreamProtocol, "responses");
    assert.equal(profile.model, draft("E2E inactive editing").model);
    assert.deepEqual(await clientSnapshot("codex"), before);
    const original = await readOptional(profile.filePath);
    await editProvider(profile.name);
    await fill("主模型", "discarded-model");
    await clickButton("取消", await $('form[aria-label="编辑供应商"]'));
    assert.equal(await readOptional(profile.filePath), original);
    await editProvider(profile.name);
    await fill("名称", "E2E inactive renamed");
    await fill("主模型", "e2e-edited-model");
    await clickButton("保存供应商");
    await providerRow("E2E inactive renamed");
    assert.equal((await jsonFile(profile.filePath)).model, "e2e-edited-model");
    assert.deepEqual(await clientSnapshot("codex"), before);
  }],

  ["Codex preview and confirmation cancellations write nothing; confirmed native switch writes config/auth and exact pre-switch backups", async () => {
    const profile = await createProvider(draft("E2E direct Codex"));
    const before = await clientSnapshot("codex");
    const backupsBefore = await backupRecords();
    let preview = await previewProvider(profile.name);
    assert(!(await preview.getText()).includes(profile.apiKey), "Preview must redact the synthetic credential");
    await clickButton("取消", preview);
    assert.deepEqual(await clientSnapshot("codex"), before);
    assert.deepEqual(await backupRecords(), backupsBefore);
    preview = await previewProvider(profile.name);
    await clickButton("确认切换", preview);
    await clickButton("取消", await $('[role="dialog"][aria-label="确认切换"]'));
    assert.deepEqual(await clientSnapshot("codex"), before);
    assert.deepEqual(await backupRecords(), backupsBefore);
    await confirmProviderSwitch();
    await waitForFile(clientFile("codex"), (text) => text?.includes(profile.baseUrl));
    const after = await readOptional(clientFile("codex"));
    assert.match(after, /model_provider\s*=\s*"agent_switchboard"/);
    assert.match(after, /\[model_providers\.agent_switchboard\]/);
    assert.match(after, /supports_websockets\s*=\s*false/);
    assert.match(after, /requires_openai_auth\s*=\s*true/);
    assert(!after.includes("openai_base_url"));
    assert.match(after, /host_owned_e2e\s*=\s*"keep-codex"/);
    assert(after.includes(profile.model));
    assert(!after.includes(profile.apiKey));
    const auth = await jsonFile(path.join(readSandbox().codex, "auth.json"));
    assert.equal(auth.OPENAI_API_KEY, profile.apiKey);
    assert.equal(auth.auth_mode, "apikey");
    assert.equal(auth.e2eHostSentinel, "keep-auth");
    const added = (await backupRecords()).filter((record) => !backupsBefore.some((old) => old.id === record.id));
    const configBackup = added.find((record) => record.targetPath === clientFile("codex"));
    assert(configBackup, "Switch must leave an external configuration backup record");
    assert.equal(await readOptional(configBackup.backupPath), before.config);
    const authBackup = added.find((record) => record.linkedBackupId === configBackup.id);
    assert(authBackup, "Codex authentication backup must be linked to its configuration backup");
    assert.equal(await readOptional(authBackup.backupPath), before.auth);
  }],

  ["active provider editing cancels without writes and confirmed save applies both the stored model and the real client model", async () => {
    const profile = await createProvider(draft("E2E active edit"));
    await switchProvider(profile.name);
    const before = await clientSnapshot("codex");
    const stored = await readOptional(profile.filePath);
    const backupsBefore = await backupRecords();
    await editProvider(profile.name);
    await fill("主模型", "e2e-active-edited-model");
    await clickButton("保存供应商");
    let sheet = await $('[role="dialog"][aria-label="确认保存并应用"]');
    await sheet.waitForDisplayed();
    await clickButton("取消", sheet);
    assert.equal(await readOptional(profile.filePath), stored);
    assert.deepEqual(await clientSnapshot("codex"), before);
    assert.deepEqual(await backupRecords(), backupsBefore);
    await clickButton("保存供应商");
    sheet = await $('[role="dialog"][aria-label="确认保存并应用"]');
    await sheet.waitForDisplayed();
    await clickButton("确认保存并应用", sheet);
    await providerRow(profile.name);
    assert.equal((await jsonFile(profile.filePath)).model, "e2e-active-edited-model");
    assert.match(await readOptional(clientFile("codex")), /model\s*=\s*"e2e-active-edited-model"/);
    assert.equal(await readOptional(path.join(readSandbox().state, "configuration", "save-journal.json")), null);
  }],

  ["external client changes after preview are shown as an error and survive a confirmed switch byte for byte", async () => {
    const profile = await createProvider(draft("E2E stale switch"));
    await previewProvider(profile.name);
    const before = await clientSnapshot("codex");
    const backupsBefore = await backupRecords();
    const external = `${before.config}\n# outside-process change must survive\n`;
    await writeFile(clientFile("codex"), external);
    await confirmProviderSwitch();
    await waitForText("配置在预览之后被外部修改，已阻止切换");
    const after = await clientSnapshot("codex");
    assert.equal(after.config, external);
    assert.equal(after.auth, before.auth);
    assert.equal(after.history, before.history);
    assert.deepEqual(await backupRecords(), backupsBefore);
    await expect($('section[aria-label="变更预览"]')).not.toBeDisplayed();
  }],

  ["external provider edits reject stale UI saves without replacing either provider or real client files", async () => {
    const profile = await createProvider(draft("E2E stale provider save"));
    await editProvider(profile.name);
    await fill("主模型", "must-not-be-saved");
    const before = await clientSnapshot("codex");
    const { filePath, ...persisted } = profile;
    const external = JSON.stringify({ ...persisted, notes: "outside-process revision" }, null, 2);
    await writeFile(filePath, external);
    await clickButton("保存供应商");
    await waitForText("供应商文件已被外部修改");
    assert.equal(await readOptional(filePath), external);
    assert.deepEqual(await clientSnapshot("codex"), before);
    await expect($('input[aria-label="主模型"]')).toHaveValue("must-not-be-saved");
    await clickButton("取消", await $('form[aria-label="编辑供应商"]'));
    await refreshConfiguration();
  }],

  ["Claude native switch preserves host JSON fields and unrelated environment entries", async () => {
    const profile = await createProvider(draft("E2E direct Claude", "claude"));
    const before = await clientSnapshot("claude");
    await switchProvider(profile.name);
    const written = await jsonFile(clientFile("claude"));
    assert.equal(written.env.ANTHROPIC_BASE_URL, profile.baseUrl);
    assert.equal(written.env.ANTHROPIC_AUTH_TOKEN, profile.apiKey);
    assert.equal(written.env.E2E_HOST_ENV, "keep-env");
    assert.equal(written.e2eHostSentinel, "keep-claude");
    const backup = (await backupRecords()).find((record) => record.targetPath === clientFile("claude"));
    assert(backup);
    assert.equal(await readOptional(backup.backupPath), before.config);
  }],

  ["selecting an existing offline official Codex profile removes the relay URL/API key and retains host-owned authentication fields", async () => {
    await navigate("供应商");
    await selectClient("codex", "供应商客户端");
    await switchProvider(officialName);
    const config = await readOptional(clientFile("codex"));
    assert.match(config, /model_provider\s*=\s*"openai"/);
    assert(!config.includes("openai_base_url"));
    assert(!config.includes("[model_providers.agent_switchboard]"));
    assert.match(config, /host_owned_e2e\s*=\s*"keep-codex"/);
    const auth = await jsonFile(path.join(readSandbox().codex, "auth.json"));
    assert.equal(auth.auth_mode, "chatgpt");
    assert.equal(auth.OPENAI_API_KEY, null);
    assert.equal(auth.e2eHostSentinel, "keep-auth");
    const official = (await providerFiles("codex")).find((profile) => profile.name === officialName);
    assert.equal(official.apiKey, "");
    assert.equal(official.baseUrl, null);
  }],

  ["keyboard reorder supports Escape cancellation, persists visible order, and rejects outside-process revisions without partial reorder", async () => {
    const first = await createProvider(draft("E2E order first"));
    const second = await createProvider(draft("E2E order second"));
    const before = await providerSnapshot("codex");
    await reorderOneUp(second.name, true);
    assert.deepEqual(await providerSnapshot("codex"), before);
    await reorderOneUp(second.name);
    await browser.waitUntil(async () => {
      const files = await providerFiles("codex");
      return files.findIndex((file) => file.id === second.id) < files.findIndex((file) => file.id === first.id);
    }, { timeout: 10000, timeoutMsg: "Keyboard reorder did not persist provider positions" });
    const ordered = await providerFiles("codex");
    const visible = await $$('[aria-label="供应商列表"] .asb-row-name').map((row) => row.getText());
    assert.deepEqual(visible, ordered.map((file) => file.name));
    const file = await jsonFile(first.filePath);
    await writeFile(first.filePath, JSON.stringify({ ...file, notes: "external reorder conflict" }, null, 2));
    const external = await providerSnapshot("codex");
    await reorderOneUp(first.name);
    await waitForText("供应商文件已被外部修改");
    assert.deepEqual(await providerSnapshot("codex"), external);
    await refreshConfiguration();
  }],

  ["delete cancellation retains a provider; a stale delete preserves the external file; refreshed confirmation removes only the provider", async () => {
    const profile = await createProvider(draft("E2E delete target"));
    const before = await clientSnapshot("codex");
    const original = await readOptional(profile.filePath);
    let sheet = await requestDelete(profile.name);
    await clickButton("取消", sheet);
    assert.equal(await readOptional(profile.filePath), original);
    sheet = await requestDelete(profile.name);
    const external = JSON.stringify({ ...JSON.parse(original), notes: "external delete conflict" }, null, 2);
    await writeFile(profile.filePath, external);
    await clickButton("确认删除", sheet);
    await waitForText("供应商文件已被外部修改");
    assert.equal(await readOptional(profile.filePath), external);
    assert.deepEqual(await clientSnapshot("codex"), before);
    await refreshConfiguration();
    await navigate("供应商");
    await selectClient("codex", "供应商客户端");
    sheet = await requestDelete(profile.name);
    await clickButton("确认删除", sheet);
    await waitForFile(profile.filePath, (content) => content === null);
    assert.deepEqual(await clientSnapshot("codex"), before);
  }],
];

describe("Providers: visible desktop operations and actual isolated configuration files", () => {
  before(seedExistingOfficialAndHostFields);
  for (const [name, scenario] of providerCases) it(name, scenario);
  it("Responses controls clear incompatible capabilities and native WebSocket support is projected explicitly", responsesControlsAndProjection);
  it("the visible model query preserves an explicit API prefix and saving a full request endpoint is visibly rejected", explicitApiRootAndEndpointRejection);
});
