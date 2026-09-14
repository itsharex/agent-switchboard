import assert from "node:assert/strict";
import { writeFile } from "node:fs/promises";
import path from "node:path";
import { clickButton, fill, navigate, openExtensionSection, openSettingsSection, readSandbox, restartDesktop, waitForText } from "../support/ui.mjs";
import {
  activateProvider, backupRecords, clientFile, clientSnapshot, confirmProviderSwitch, createProvider, ensureSandboxPath,
  jsonFile, providerRow, radio, readOptional, selectClient, switchProvider, waitForFile,
} from "../support/provider-ui.mjs";

function clientSettingsFile(app) {
  return ensureSandboxPath(path.join(readSandbox().state, "configuration", "client-settings", `${app}.json`));
}

async function openClientSettings(app) {
  await openSettingsSection("客户端偏好");
  await selectClient(app, "客户端偏好客户端");
  await $('button=保存客户端设置').waitForDisplayed();
}

async function nativeProvider(app) {
  assert(process.env.ASB_E2E_UPSTREAM_URL);
  return createProvider({
    app, name: `E2E clientSettings ${app}`, apiKey: "e2e-clientSettings-key", model: "e2e-clientSettings-model",
    baseUrl: `${process.env.ASB_E2E_UPSTREAM_URL}${app === "codex" ? "/v1" : ""}`,
    protocol: app === "codex" ? "responses" : "anthropicMessages",
  });
}

async function discardClientSettingsPreview() {
  await openClientSettings("codex");
  const clientSettingsBefore = await readOptional(clientSettingsFile("codex"));
  const clientBefore = await clientSnapshot("codex");
  const group = await $('[role="radiogroup"][aria-label="批准策略"]');
  const originalLabel = await group.$("label.is-active").getText();
  const changedLabel = originalLabel === "从不" ? "按请求" : "从不";
  await radio("批准策略", changedLabel);
  await clickButton("查看客户端配置预览");
  await $('[role="region"][aria-label="客户端配置预览"]').waitForDisplayed();
  await expect($('[role="region"][aria-label="客户端配置预览"]')).toHaveText(/approval_policy/);
  await clickButton("收起客户端配置预览");
  assert.equal(await readOptional(clientSettingsFile("codex")), clientSettingsBefore);
  assert.deepEqual(await clientSnapshot("codex"), clientBefore);
  await restartDesktop();
  await openClientSettings("codex");
  assert.equal(await $('[role="radiogroup"][aria-label="批准策略"] label.is-active').getText(), originalLabel);
  assert.equal(await readOptional(clientSettingsFile("codex")), clientSettingsBefore);
  assert.deepEqual(await clientSnapshot("codex"), clientBefore);
}

async function saveCodexClientSettingsAndApply() {
  const before = await clientSnapshot("codex");
  await openClientSettings("codex");
  await radio("批准策略", "从不");
  await radio("终端通知（回合结束时）", "开启");
  await clickButton("查看客户端配置预览");
  await expect($('[role="region"][aria-label="客户端配置预览"]')).toHaveText(/notifications/);
  await clickButton("保存客户端设置");
  await waitForText("已保存，请在供应商页选择并启用供应商后生效");
  const clientSettings = await jsonFile(clientSettingsFile("codex"));
  assert.deepEqual(clientSettings.settings.approval_policy, { mode: "explicit", value: "never" });
  assert.deepEqual(clientSettings.settings["tui.notifications"], { mode: "explicit", value: true });
  assert.deepEqual(await clientSnapshot("codex"), before);
  const provider = await nativeProvider("codex");
  await switchProvider(provider.name);
  const actual = await readOptional(clientFile("codex"));
  assert.match(actual, /approval_policy\s*=\s*"never"/);
  assert.match(actual, /notifications\s*=\s*true/);
  assert(actual.includes('[mcp_servers.e2e-native-mcp]'));
  assert(actual.includes('ASB_E2E_PUBLIC = "host-preserved"'));
  assert(actual.includes('# E2E native host configuration'));
}

async function saveClaudeClientSettingsAndApply() {
  const before = await clientSnapshot("claude");
  await openClientSettings("claude");
  await radio("输出自动滚动", "关闭");
  await radio("通知渠道", "关闭通知");
  await clickButton("查看客户端配置预览");
  await expect($('[role="region"][aria-label="客户端配置预览"]')).toHaveText(/autoScrollEnabled/);
  await clickButton("保存客户端设置");
  await waitForText("已保存，请在供应商页选择并启用供应商后生效");
  const clientSettings = await jsonFile(clientSettingsFile("claude"));
  assert.deepEqual(clientSettings.settings.autoScrollEnabled, { mode: "explicit", value: false });
  assert.deepEqual(clientSettings.settings.preferredNotifChannel, { mode: "explicit", value: "notifications_disabled" });
  assert.deepEqual(await clientSnapshot("claude"), before);
  const provider = await nativeProvider("claude");
  await switchProvider(provider.name);
  const written = await jsonFile(clientFile("claude"));
  assert.equal(written.autoScrollEnabled, false);
  assert.equal(written.preferredNotifChannel, "notifications_disabled");
  assert.equal(written.env.ASB_E2E_HOST, "preserve-me");
  assert.deepEqual(written.permissions, JSON.parse(before.config).permissions);
}

async function previewAndApplySavedPreferences() {
  const providerName = "E2E clientSettings codex";
  await openClientSettings("codex");
  const before = await clientSnapshot("codex");
  await radio("批准策略", "按请求");
  await clickButton("保存并预览应用");
  const preview = await $('section[aria-label="变更预览"]');
  await preview.waitForDisplayed();
  await expect(preview).toHaveText(/approval_policy/);
  assert.deepEqual(await clientSnapshot("codex"), before);
  assert.deepEqual((await jsonFile(clientSettingsFile("codex"))).settings.approval_policy,
    { mode: "explicit", value: "on-request" });
  await expect(preview.$("button=确认切换")).not.toExist();
  await expect(preview.$("button=取消")).not.toExist();
  await expect($('[role="dialog"][aria-label="确认切换"]')).not.toExist();
  await clickButton(`收起 ${providerName} 预览`, await providerRow(providerName));
  await preview.waitForDisplayed({ reverse: true });
  assert.deepEqual(await clientSnapshot("codex"), before);
  await openClientSettings("codex");
  await clickButton("保存并预览应用");
  await preview.waitForDisplayed();
  await expect($('[role="dialog"][aria-label="确认切换"]')).not.toExist();
  assert.deepEqual(await clientSnapshot("codex"), before);
  await activateProvider(providerName);
  assert.deepEqual(await clientSnapshot("codex"), before);
  await confirmProviderSwitch();
  await waitForFile(clientFile("codex"), (text) => /approval_policy\s*=\s*"on-request"/.test(text));
  await openClientSettings("codex");
  await waitForText('已应用：真实配置与「E2E clientSettings codex」一致');
  await radio("批准策略", "从不");
  await navigate("会话");
  await openClientSettings("codex");
  assert.equal(await $('[role="radiogroup"][aria-label="批准策略"] label.is-active').getText(), "从不");
  await restartDesktop();
}

async function rejectStaleClientSettingsSave() {
  await openClientSettings("codex");
  await radio("批准策略", "从不");
  const before = await clientSnapshot("codex");
  const original = await readOptional(clientSettingsFile("codex"));
  assert(original, "The successful client-settings case must have created its typed store");
  const external = `${original}\n`;
  await writeFile(clientSettingsFile("codex"), external);
  await clickButton("保存客户端设置");
  await waitForText("保存失败，修改已保留");
  await waitForText("客户端设置已更新，请重新加载后再保存");
  assert.equal(await readOptional(clientSettingsFile("codex")), external);
  assert.deepEqual(await clientSnapshot("codex"), before);
  assert.equal(await $('[role="radiogroup"][aria-label="批准策略"] label.is-active').getText(), "从不");
  await restartDesktop();
}

async function promptDocumentScenario(app) {
  const name = app === "codex" ? "AGENTS.md" : "CLAUDE.md";
  const target = ensureSandboxPath(path.join(readSandbox()[app], name));
  const backupDirectory = ensureSandboxPath(path.join(readSandbox().state, "backups", "prompts"));
  const before = await readOptional(target);
  const clientBefore = await clientSnapshot(app);
  const backupsBefore = await backupRecords(backupDirectory);
  await openExtensionSection("全局指令");
  await selectClient(app, "全局指令客户端");
  await expect($(`textarea[aria-label="${name} 内容"]`)).toHaveValue(before);
  await fill(`${name} 内容`, "Discard this unsaved instruction");
  await clickButton("放弃草稿");
  assert.equal(await readOptional(target), before);
  assert.deepEqual(await backupRecords(backupDirectory), backupsBefore);
  const saved = `# E2E ${app} global instructions\nUse the local deterministic fixture.\n`;
  await fill(`${name} 内容`, saved);
  await clickButton(`保存 ${name}`);
  await waitForFile(target, (text) => text === saved);
  const backupsAfter = await backupRecords(backupDirectory);
  const backup = backupsAfter.find((record) => record.targetPath === target && record.reason === "prompt-management");
  assert(backup, "Saving a global instruction must create its real executor backup");
  assert.equal(await readOptional(backup.backupPath), before);
  assert.deepEqual(await clientSnapshot(app), clientBefore);
  await fill(`${name} 内容`, "This stale draft must stay only in the editor");
  const external = `${saved}\nExternal process owns this final line.\n`;
  await writeFile(target, external);
  await clickButton(`保存 ${name}`);
  await waitForText("全局提示词文档已在读取后被外部修改");
  assert.equal(await readOptional(target), external);
  assert.deepEqual(await backupRecords(backupDirectory), backupsAfter);
  await expect($(`textarea[aria-label="${name} 内容"]`)).toHaveValue("This stale draft must stay only in the editor");
  await clickButton("放弃草稿");
  await clickButton("重新读取");
  await expect($(`textarea[aria-label="${name} 内容"]`)).toHaveValue(external);
}

describe("Client settings and global instruction files through the real desktop UI", () => {
  it("folded preview and abandoned draft do not persist, including across a real process restart", discardClientSettingsPreview);
  it("Codex client settings persist separately and reach config.toml only after the supplier is activated", saveCodexClientSettingsAndApply);
  it("Claude client settings persist separately and preserve host JSON when a supplier is activated", saveClaudeClientSettingsAndApply);
  it("saving preferences opens a fresh preview, cancel writes no client file, and confirmation applies it", previewAndApplySavedPreferences);
  it("outside-process client-settings store changes produce a visible error and preserve the unsaved draft and external file", rejectStaleClientSettingsSave);
  for (const app of ["codex", "claude"]) {
    it(`${app} global instructions: discard writes nothing, save leaves an exact backup, stale save preserves the external revision`, async () => {
      await promptDocumentScenario(app);
    });
  }
});
