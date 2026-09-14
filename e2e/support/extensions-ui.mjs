import assert from 'node:assert/strict';
import { readFile, readdir, stat } from 'node:fs/promises';
import path from 'node:path';
import { clickButton, readSandbox, visibleElement, xpathText } from './ui.mjs';

export async function extensionRecords(collection) {
  assert.ok(['definitions', 'bindings', 'baselines', 'history'].includes(collection));
  const directory = path.join(readSandbox().state, 'extensions', collection);
  const files = await readdir(directory).catch((error) => {
    if (error.code === 'ENOENT') return [];
    throw error;
  });
  return Promise.all(files.filter((name) => name.endsWith('.json')).map(async (name) =>
    JSON.parse(await readFile(path.join(directory, name), 'utf8'))));
}

export async function optionalText(file) {
  const relative = path.relative(readSandbox().root, path.resolve(file));
  assert.ok(relative && !relative.startsWith('..') && !path.isAbsolute(relative));
  return readFile(file, 'utf8').catch((error) => {
    if (error.code === 'ENOENT') return null;
    throw error;
  });
}

export async function extensionDialog(title) {
  return visibleElement(`[role="dialog"][aria-label=${JSON.stringify(title)}]`);
}

export async function closeExtensionDialog(title) {
  const dialog = await extensionDialog(title);
  await clickButton('关闭窗口', dialog);
  await dialog.waitForDisplayed({ reverse: true });
}

export async function openDiscovery() {
  await clickButton('从本机发现');
  const dialog = await extensionDialog('从本机发现');
  await (await dialog.$('ul[aria-label="本机发现的扩展"]')).waitForDisplayed();
  return dialog;
}

export async function scanExtensions() {
  const dialog = await extensionDialog('从本机发现');
  const button = await clickButton('重新扫描', dialog);
  await button.waitForEnabled();
  return dialog;
}

export async function discoveryRow(name, client = 'Codex') {
  const dialog = await extensionDialog('从本机发现');
  const label = `选择 ${name} 的 ${client} 安装`;
  return visibleElement(`.//ul[@aria-label='本机发现的扩展']/li[.//input[@aria-label=${xpathText(label)}]]`, dialog);
}

export async function waitForToast(pattern, timeout = 20000) {
  await browser.waitUntil(async () => {
    for (const toast of await $$('.asb-toast-body')) {
      if (await toast.isDisplayed() && pattern.test(await toast.getText())) return true;
    }
    return false;
  }, { timeout, timeoutMsg: `Expected toast ${pattern} did not appear` });
}

export async function libraryRow(name) {
  return visibleElement(`.//ul[@aria-label='扩展列表']/li[.//button[@aria-label=${xpathText(`管理 ${name}`)}]]`);
}

async function rowToggle(name, client) {
  const row = await libraryRow(name);
  return row.$(`button[data-client=${JSON.stringify(client)}]`);
}

/** One click deploys or undeploys immediately. The only write that stops
 * for an explicit answer is a sensitive connection write; when one is
 * pending its confirmation sheet is returned, otherwise the toggle's
 * settled state means the transaction already committed. */
export async function toggleDeployment(name, client) {
  const toggle = await rowToggle(name, client);
  await toggle.waitForEnabled();
  await toggle.scrollIntoView({ block: 'center' });
  const before = await toggle.getAttribute('aria-pressed');
  await toggle.click();
  await browser.waitUntil(async () => {
    if (await (await $('[role="dialog"][aria-label$="（写入敏感数据）"]')).isExisting()) return true;
    return (await (await rowToggle(name, client)).getAttribute('aria-pressed')) !== before;
  }, { timeout: 20000, timeoutMsg: `Toggle for ${name}/${client} did not settle` });
  const sheet = await $('[role="dialog"][aria-label$="（写入敏感数据）"]');
  if (await sheet.isExisting()) return sheet;
  await waitForToast(/已应用扩展变更/);
  return null;
}

export async function confirmSensitiveWrite(sheet, confirm) {
  await clickButton(confirm ? '确认写入' : '取消', sheet);
  await sheet.waitForDisplayed({ reverse: true });
  if (confirm) await waitForToast(/已应用扩展变更|变更未能应用|应用失败/);
}

export async function openHistory() {
  await clickButton('操作历史');
  return extensionDialog('操作历史');
}

export async function importNativeSelection(name, client = 'Codex') {
  const dialog = await extensionDialog('从本机发现');
  const selectAll = await dialog.$('input[aria-label="全选"]');
  if (await selectAll.isSelected()) await (await selectAll.$('..')).click();
  const row = await discoveryRow(name, client);
  const checkbox = await row.$('input[type="checkbox"]');
  if (!(await checkbox.isSelected())) await (await checkbox.$('..')).click();
  await clickButton('导入所选（1）', dialog);
  await waitForToast(/已导入本机扩展/);
  await closeExtensionDialog('从本机发现');
}

export async function restoreLatest(name) {
  const dialog = await openHistory();
  const row = await visibleElement(`.//li[contains(@class,'asb-ext-history-item')][.//span[normalize-space(.)=${xpathText(name)}]]`, dialog);
  await clickButton('恢复', row);
  await waitForToast(/已应用扩展变更|变更未能应用|应用失败/);
  await closeExtensionDialog('操作历史');
}

export async function assertOperationEvidence(kind) {
  const sandbox = readSandbox();
  const snapshots = await extensionRecords('history');
  const snapshot = snapshots.filter((item) => item.record.resources.some((resource) => resource.operation === kind))
    .sort((a, b) => b.record.createdAt.localeCompare(a.record.createdAt))[0];
  assert.ok(snapshot, `No persisted ${kind} operation`);
  assert.ok(snapshot.completedSteps.length > 0, 'Operation must record real filesystem steps');
  for (const { step } of snapshot.completedSteps) {
    for (const file of [step.path, step.targetDir, step.backupReference].filter(Boolean)) {
      const relative = path.relative(sandbox.root, path.resolve(file));
      assert.ok(relative && !relative.startsWith('..') && !path.isAbsolute(relative), `Unisolated operation path: ${file}`);
    }
    if (step.backupReference) assert.ok((await stat(step.backupReference)).isFile() || (await stat(step.backupReference)).isDirectory());
  }
  const journal = await optionalText(path.join(sandbox.state, 'extensions', 'transactions', snapshot.record.id, 'journal.jsonl'));
  if (journal !== null) {
    const entries = journal.trim().split('\n').map((line) => JSON.parse(line));
    assert.equal(entries[0].phase, 'prepared');
    assert.equal(entries.at(-1).phase, 'committed');
  }
  return snapshot;
}
