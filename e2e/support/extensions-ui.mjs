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
  await (await dialog.$('table[aria-label="本机发现的扩展"]')).waitForDisplayed();
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
  return visibleElement(`.//table[@aria-label='本机发现的扩展']//tbody/tr[td//*[normalize-space(.)=${xpathText(name)}] and td[normalize-space(.)=${xpathText(client)}]]`, dialog);
}

export async function libraryRow(name) {
  return visibleElement(`.//ul[@aria-label='扩展列表']/li[.//button[@aria-label=${xpathText(`管理 ${name}`)}]]`);
}

export async function toggleDeployment(name, client) {
  const row = await libraryRow(name);
  const toggle = await row.$(`button[data-client=${JSON.stringify(client)}]`);
  await toggle.waitForEnabled();
  await toggle.scrollIntoView({ block: 'center' });
  await toggle.click();
  return visibleElement('[role="dialog"][aria-label$="预览"]');
}

export async function finishPlan(confirm) {
  const dialog = await visibleElement('[role="dialog"][aria-label$="预览"]');
  await clickButton(confirm ? '确认应用' : '取消', dialog);
  await dialog.waitForDisplayed({ reverse: true });
}

export async function openHistory() {
  await clickButton('更多扩展操作');
  const item = await visibleElement('.//*[@role="menuitem" and normalize-space(.)="操作历史"]');
  await item.click();
  return extensionDialog('操作历史');
}

export async function restoreLatest(name, confirm) {
  const dialog = await openHistory();
  const row = await visibleElement(`.//li[contains(@class,'asb-ext-history-item')][.//span[normalize-space(.)=${xpathText(name)}]]`, dialog);
  await clickButton('恢复', row);
  await finishPlan(confirm);
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
