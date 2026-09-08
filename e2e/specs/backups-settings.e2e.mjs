import assert from "node:assert/strict";
import { chmod, readFile, readdir, rm, writeFile } from "node:fs/promises";
import path from "node:path";
import {
  clickButton, fill, openDiagnosticsSection, openSettingsSection, readSandbox, restartDesktop, selectOption, waitForText, xpathText,
} from "../support/ui.mjs";
import {
  backupRecords, clientFile, clientSnapshot, createProvider, ensureSandboxPath,
  radio, readOptional, refreshConfiguration, switchProvider, waitForFile,
} from "../support/provider-ui.mjs";

async function makeProvider(name, app = "codex") {
  assert(process.env.ASB_E2E_UPSTREAM_URL);
  return createProvider({
    app, name, model: `e2e-${name.replaceAll(" ", "-")}`, apiKey: "e2e-backup-key",
    baseUrl: `${process.env.ASB_E2E_UPSTREAM_URL}${app === "codex" ? "/v1" : ""}`,
    protocol: app === "codex" ? "responses" : "anthropicMessages",
  });
}

async function newestConfigBackup(app) {
  const record = (await backupRecords()).find((backup) => backup.targetPath === clientFile(app));
  assert(record, `Expected an observable ${app} backup on disk`);
  return record;
}

async function backupRow(record) {
  const row = await $(`//table[@aria-label="备份历史"]//tr[.//time[@datetime=${xpathText(record.createdAt)}] and .//td[normalize-space(.)=${xpathText(record.contentHash.slice(0, 12))}]]`);
  await row.waitForDisplayed();
  return row;
}

async function requestRestore(record) {
  await clickButton("恢复", await backupRow(record));
  const sheet = await $('[role="dialog"][aria-label="恢复备份"]');
  await sheet.waitForDisplayed();
  return sheet;
}

async function restoreAndUndoBytes() {
  const profile = await makeProvider("E2E byte restore");
  const original = await clientSnapshot("codex");
  await switchProvider(profile.name);
  const switched = await clientSnapshot("codex");
  const backup = await newestConfigBackup("codex");
  await openSettingsSection("备份与恢复");
  await clickButton("查看差异", await backupRow(backup));
  await expect($('[aria-label="当前文件与备份的差异"]')).toHaveText(/model/);
  const beforeCancel = await backupRecords();
  let sheet = await requestRestore(backup);
  await clickButton("取消", sheet);
  assert.deepEqual(await clientSnapshot("codex"), switched);
  assert.deepEqual(await backupRecords(), beforeCancel);
  sheet = await requestRestore(backup);
  await clickButton("确认恢复", sheet);
  await waitForFile(clientFile("codex"), (text) => text === original.config);
  await waitForText("已恢复备份");
  assert.equal(await readOptional(path.join(readSandbox().codex, "auth.json")), original.auth);
  const preRestore = await newestConfigBackup("codex");
  assert.equal(preRestore.reason, "restore-precheck");
  assert.equal(await readOptional(preRestore.backupPath), switched.config);
  await clickButton("撤回上一次切换");
  sheet = await $('[role="dialog"][aria-label="撤回上一次切换"]');
  await clickButton("取消", sheet);
  assert.equal(await readOptional(clientFile("codex")), original.config);
  await clickButton("撤回上一次切换");
  sheet = await $('[role="dialog"][aria-label="撤回上一次切换"]');
  await clickButton("确认撤回", sheet);
  await waitForFile(clientFile("codex"), (text) => text === switched.config);
  await waitForText("已撤回上一次切换");
  assert.equal(await readOptional(path.join(readSandbox().codex, "auth.json")), switched.auth);
}

async function firstWriteUndoRestoresAbsence() {
  const target = clientFile("claude");
  await rm(ensureSandboxPath(target), { force: true });
  await refreshConfiguration();
  const provider = await makeProvider("E2E first Claude write", "claude");
  await switchProvider(provider.name);
  assert.notEqual(await readOptional(target), null);
  const backup = await newestConfigBackup("claude");
  assert.equal(backup.targetExisted, false);
  assert.equal(await readOptional(backup.backupPath), "");
  await openSettingsSection("备份与恢复");
  await clickButton("撤回上一次切换");
  await clickButton("确认撤回", await $('[role="dialog"][aria-label="撤回上一次切换"]'));
  await waitForFile(target, (text) => text === null, "Undo of the first write must remove settings.json");
  await waitForText("已撤回上一次切换");
  assert.equal(await readOptional(target), null);
}

async function corruptBackupRejectsRestore() {
  const profile = await makeProvider("E2E corrupt backup", "codex");
  await switchProvider(profile.name);
  const backup = await newestConfigBackup("codex");
  const originalBackup = await readOptional(backup.backupPath);
  const before = await clientSnapshot("codex");
  const recordsBefore = await backupRecords();
  await openSettingsSection("备份与恢复");
  const sheet = await requestRestore(backup);
  try {
    await writeFile(ensureSandboxPath(backup.backupPath), `${originalBackup}\n# externally corrupted backup\n`);
    await clickButton("确认恢复", sheet);
    await waitForText("配置在预览之后被外部修改，已阻止切换");
    assert.deepEqual(await clientSnapshot("codex"), before);
    assert.deepEqual(await backupRecords(), recordsBefore);
    await clickButton("查看差异", await backupRow(backup));
    await waitForText("备份内容与记录哈希不符，拒绝生成差异");
  } finally {
    await writeFile(ensureSandboxPath(backup.backupPath), originalBackup);
  }
}

function settingsPath() {
  return ensureSandboxPath(path.join(readSandbox().state, "settings.json"));
}

async function savedSetting(field, value) {
  await waitForFile(settingsPath(), (text) => text !== null && JSON.parse(text)[field] === value);
  await browser.waitUntil(async () => !(await $('[aria-label="处理中"]').isDisplayed()), { timeout: 15000 });
}

async function persistAppearanceAndRestart() {
  const codexBefore = await clientSnapshot("codex");
  const claudeBefore = await clientSnapshot("claude");
  await openSettingsSection("应用偏好");
  const beforePicker = await readOptional(settingsPath());
  await clickButton("选择界面字体");
  await fill("搜索字体", "E2E deliberately nonexistent font");
  await waitForText("没有找到相关字体");
  await browser.keys("Escape");
  assert.equal(await readOptional(settingsPath()), beforePicker);
  await radio("界面主题", "深色");
  await savedSetting("theme", "dark");
  await radio("动态效果", "减少动态效果");
  await savedSetting("motion", "reduce");
  await radio("点击关闭按钮时", "退出应用");
  await savedSetting("closeBehavior", "exit");
  await clickButton("选择界面字体");
  await fill("搜索字体", "Arial");
  const font = await $('//button[@role="option" and .//span[normalize-space(.)="Arial"]]');
  await font.waitForDisplayed();
  await font.click();
  await savedSetting("interfaceFont", "Arial");
  await $('//label[input[@role="switch" and @aria-label="窗口始终置顶"]]').click();
  await savedSetting("alwaysOnTop", true);
  await openDiagnosticsSection("运行日志");
  await selectOption("记录级别", "调试");
  await savedSetting("runtimeLogLevel", "debug");
  await restartDesktop();
  await openSettingsSection("应用偏好");
  await expect($('html')).toHaveAttribute("data-theme", "dark");
  await expect($('html')).toHaveAttribute("data-motion", "reduce");
  await expect($('html')).toHaveAttribute("style", /Arial/);
  await expect($('button[aria-label="选择界面字体"]')).toHaveText(/Arial/);
  await expect($('input[aria-label="窗口始终置顶"]')).toBeSelected();
  assert.equal(await $('[role="radiogroup"][aria-label="点击关闭按钮时"] label.is-active').getText(), "退出应用");
  await openDiagnosticsSection("运行日志");
  await expect($('[role="combobox"][aria-label="记录级别"]')).toHaveText("调试");
  assert.deepEqual(await clientSnapshot("codex"), codexBefore);
  assert.deepEqual(await clientSnapshot("claude"), claudeBefore);
}

async function readOnlySettingsRetainState() {
  await openSettingsSection("应用偏好");
  const before = await readOptional(settingsPath());
  assert(before);
  const appearance = await $('html').getAttribute("data-theme");
  await chmod(settingsPath(), 0o444);
  try {
    await radio("界面主题", "浅色");
    await waitForText("无法原子保存应用设置");
    assert.equal(await readOptional(settingsPath()), before);
    await expect($('html')).toHaveAttribute("data-theme", appearance);
    assert.equal(await $('[role="radiogroup"][aria-label="界面主题"] label.is-active').getText(), "深色");
  } finally {
    await chmod(settingsPath(), 0o666);
  }
}

async function runtimeLogs() {
  const directory = ensureSandboxPath(readSandbox().logs);
  const names = (await readdir(directory)).filter((name) => /^agent-switchboard(?:_.*)?\.log$/.test(name));
  const texts = await Promise.all(names.map((name) => readFile(path.join(directory, name), "utf8")));
  return texts.flatMap((text) => text.split(/\r?\n/).filter(Boolean).map((line) => JSON.parse(line)))
    .sort((left, right) => right.at.localeCompare(left.at)).slice(0, 500);
}

async function logsMatchRealEvents() {
  await openDiagnosticsSection("运行日志");
  const logsPage = await $('section[aria-label="日志"]');
  await clickButton("刷新", logsPage);
  const records = await runtimeLogs();
  assert(records.filter((record) => record.action === "appStarted").length >= 2, "The process restart must log another native startup");
  assert(records.some((record) => record.action === "configurationSwitched"));
  assert(records.some((record) => record.action === "backupRestored"));
  assert(records.some((record) => record.action === "switchUndone"));
  assert(records.some((record) => record.action === "appSettingsSaved" && record.errorCode === "app-settings-save-failed"));
  const table = await $('table[aria-label="应用运行日志"]');
  await table.waitForDisplayed();
  await browser.waitUntil(async () => (await table.$$("tbody tr")).length === records.length, { timeout: 10000 });
  const times = await table.$$("tbody tr time").map((time) => time.getAttribute("datetime"));
  assert.deepEqual(times, records.map((record) => record.at));
  assert(!JSON.stringify(records).includes("e2e-backup-key"));
  await clickButton("错误", await $('[aria-label="日志级别筛选"]'));
  const errors = records.filter((record) => record.level === "error");
  assert(errors.length > 0, "Failure cases must generate real error events");
  await browser.waitUntil(async () => (await table.$$("tbody tr")).length === errors.length, { timeout: 10000 });
  assert.deepEqual(await table.$$("tbody tr time").map((time) => time.getAttribute("datetime")), errors.map((record) => record.at));
  for (const row of await table.$$("tbody tr")) assert((await row.getText()).includes("错误"));
}

describe("Desktop backups, runtime settings and observable application logs", () => {
  it("backup preview/cancel is read-only; restore and undo restore configuration/authentication bytes exactly", restoreAndUndoBytes);
  it("undo after the first Claude write restores the file-does-not-exist state", firstWriteUndoRestoresAbsence);
  it("externally corrupted backup is visibly rejected without changing client files or write history", corruptBackupRejectsRestore);
  it("font-picker cancellation writes nothing; appearance, font, log level and pin settings survive a real process restart", persistAppearanceAndRestart);
  it("a Windows read-only settings file causes a visible save error and retains disk bytes and rendered settings", readOnlySettingsRetainState);
  it("log refresh/order/filter match the real native process event files and exclude synthetic API keys", logsMatchRealEvents);
});
