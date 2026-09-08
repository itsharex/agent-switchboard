import assert from 'node:assert/strict';
import { randomUUID } from 'node:crypto';
import { mkdir, readFile, rmdir, unlink, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { clickButton, fill, openExtensionSection, readSandbox, visibleElement, waitForText } from '../support/ui.mjs';
import {
  assertOperationEvidence, closeExtensionDialog, discoveryRow, extensionDialog,
  extensionRecords, finishPlan, libraryRow, openDiscovery, optionalText,
  restoreLatest, scanExtensions, toggleDeployment,
} from '../support/extensions-ui.mjs';

let sandbox;
let fixture;

async function waitForRejection() {
  await browser.waitUntil(async () => {
    for (const toast of await $$('.asb-toast-body')) {
      if (await toast.isDisplayed() && /计划未能应用|外部改|目标.*变|已变化|前置|应用失败/.test(await toast.getText())) return true;
    }
    return false;
  }, { timeout: 15000, timeoutMsg: 'The desktop did not expose the stale-plan rejection' });
  const pending = await $('[role="dialog"][aria-label$="预览"]');
  if (await pending.isDisplayed()) await clickButton('取消', pending);
}

async function secretDraft(name, token) {
  await clickButton('添加 MCP');
  const dialog = await extensionDialog('添加 MCP');
  await fill('服务名称（同时作为客户端服务键）', name, dialog);
  await fill('启动命令', 'cmd.exe', dialog);
  await fill('启动参数', '/d\n/c\necho e2e-secret-mcp', dialog);
  await fill('环境变量名 1', 'ASB_E2E_TOKEN', dialog);
  await fill('环境变量值 1', token, dialog);
  await (await dialog.$('.//label[input[@aria-label="环境变量 1 存入系统凭据"]]')).click();
  return dialog;
}

before(async () => {
  sandbox = readSandbox();
  fixture = JSON.parse(await readFile(path.join(sandbox.fixtures, 'manifest.json'), 'utf8'));
  await openExtensionSection('Skills');
});

describe('Real desktop extension discovery and management', () => {
  it('scans native Skills with warnings collapsed, expands them, and collapses each new scan', async () => {
    const dialog = await openDiscovery();
    await discoveryRow(fixture.skills.name);
    const summary = await dialog.$('section[aria-label="发现警告"] button[aria-expanded]');
    await expect(summary).toHaveAttribute('aria-expanded', 'false');
    await expect(dialog.$('.asb-discovery-diagnostic-list')).not.toExist();
    await summary.click();
    await expect(summary).toHaveAttribute('aria-expanded', 'true');
    await waitForText('缺少 frontmatter');
    await browser.saveScreenshot(path.join(sandbox.artifacts, 'extensions-warning-expanded.png'));
    await scanExtensions();
    await expect(summary).toHaveAttribute('aria-expanded', 'false');
    assert.equal(await optionalText(fixture.skills.manifest.file), fixture.skills.manifest.content);
    assert.equal(await optionalText(fixture.skills.invalid.file), fixture.skills.invalid.content);
    assert.deepEqual(await extensionRecords('definitions'), []);
    await closeExtensionDialog('从本机发现');
  });

  it('cancels and then confirms management of a native Skill without changing its bytes', async () => {
    await openDiscovery();
    await clickButton('管理现有安装', await discoveryRow(fixture.skills.name));
    let dialog = await extensionDialog(`管理现有安装：${fixture.skills.name}`);
    await clickButton('取消', dialog);
    assert.deepEqual(await extensionRecords('definitions'), []);
    assert.equal(await optionalText(fixture.skills.guide.file), fixture.skills.guide.content);
    await clickButton('管理现有安装', await discoveryRow(fixture.skills.name));
    dialog = await extensionDialog(`管理现有安装：${fixture.skills.name}`);
    await expect(dialog).toHaveText(expect.stringContaining('现有文件保持原样'));
    await clickButton('确认管理', dialog);
    await closeExtensionDialog(`扩展详情 ${fixture.skills.name}`);
    assert.equal(await optionalText(fixture.skills.manifest.file), fixture.skills.manifest.content);
    assert.equal(await optionalText(fixture.skills.guide.file), fixture.skills.guide.content);
    assert.ok((await extensionRecords('definitions')).some((item) => item.name === fixture.skills.name));
    assert.equal((await extensionRecords('bindings')).length, 1);
    assert.equal((await extensionRecords('baselines')).length, 1);
  });

});

describe('Real desktop Skill deployment and restoration', () => {
  it('leaves cancelled deployments absent and rejects a target changed outside the application', async () => {
    const target = fixture.skills.claudeTarget;
    await toggleDeployment(fixture.skills.name, 'claude');
    await finishPlan(false);
    assert.equal(await optionalText(path.join(target, 'SKILL.md')), null);
    const baselineBindings = await extensionRecords('bindings');
    const preview = await toggleDeployment(fixture.skills.name, 'claude');
    const foreignFile = path.join(target, 'external-change.txt');
    await mkdir(target, { recursive: true });
    await writeFile(foreignFile, 'An external owner created this after preview.\n');
    await clickButton('确认应用', preview);
    await waitForRejection();
    assert.equal(await optionalText(foreignFile), 'An external owner created this after preview.\n');
    assert.equal(await optionalText(path.join(target, 'SKILL.md')), null);
    assert.deepEqual(await extensionRecords('bindings'), baselineBindings);
    await unlink(foreignFile);
    await rmdir(target);
  });

  it('deploys actual Skill files to Claude and restores a first install to file absence', async () => {
    await toggleDeployment(fixture.skills.name, 'claude');
    await finishPlan(true);
    const manifest = path.join(fixture.skills.claudeTarget, 'SKILL.md');
    const guide = path.join(fixture.skills.claudeTarget, 'references', 'guide.md');
    assert.equal(await optionalText(manifest), fixture.skills.manifest.content);
    assert.equal(await optionalText(guide), fixture.skills.guide.content);
    const operation = await assertOperationEvidence('install');
    assert.ok(operation.completedSteps.some(({ step }) => step.kind === 'directoryDeployed' && step.originalDigest === null));
    await restoreLatest(fixture.skills.name, false);
    assert.equal(await optionalText(manifest), fixture.skills.manifest.content);
    await restoreLatest(fixture.skills.name, true);
    assert.equal(await optionalText(manifest), null);
    assert.equal(await optionalText(guide), null);
    assert.equal(await optionalText(fixture.skills.manifest.file), fixture.skills.manifest.content);
    await assertOperationEvidence('restore');
  });

});

describe('Real desktop missing Skill repair', () => {
  it('previews one-click repair, preserves an external conflict, then restores the missing Skill file', async () => {
    await unlink(fixture.skills.guide.file);
    let dialog = await openDiscovery();
    await scanExtensions();
    const repair = await visibleElement('.//button[@aria-label="修复这 1 项可修复警告"]', dialog);
    await expect(dialog.$('section[aria-label="发现警告"] button[aria-expanded]')).toHaveAttribute('aria-expanded', 'false');
    await repair.click();
    await expect(await extensionDialog('修复预览')).toHaveText(expect.stringContaining(fixture.skills.name));
    await finishPlan(false);
    assert.equal(await optionalText(fixture.skills.guide.file), null);
    await repair.click();
    const external = `${fixture.skills.manifest.content}\nExternal edit after repair preview.\n`;
    await writeFile(fixture.skills.manifest.file, external);
    await clickButton('确认应用', await extensionDialog('修复预览'));
    await waitForRejection();
    assert.equal(await optionalText(fixture.skills.manifest.file), external);
    assert.equal(await optionalText(fixture.skills.guide.file), null);
    await writeFile(fixture.skills.manifest.file, fixture.skills.manifest.content);
    dialog = await scanExtensions();
    await clickButton('修复这 1 项可修复警告', dialog);
    await finishPlan(true);
    await browser.waitUntil(async () => await optionalText(fixture.skills.guide.file) === fixture.skills.guide.content);
    await expect(dialog.$('button[aria-label="修复这 1 项可修复警告"]')).not.toExist();
    await assertOperationEvidence('repair');
    await closeExtensionDialog('从本机发现');
  });

});

describe('Real desktop MCP and system credentials', () => {
  it('takes over a native MCP entry while preserving its host document', async () => {
    await clickButton('MCP', await $('section[aria-label="扩展"]'));
    await openDiscovery();
    const before = await optionalText(fixture.host.codex.file);
    await clickButton('管理现有安装', await discoveryRow('e2e-native-mcp'));
    await clickButton('取消', await extensionDialog('管理现有安装：e2e-native-mcp'));
    assert.equal(await optionalText(fixture.host.codex.file), before);
    await clickButton('管理现有安装', await discoveryRow('e2e-native-mcp'));
    await clickButton('确认管理', await extensionDialog('管理现有安装：e2e-native-mcp'));
    await closeExtensionDialog('扩展详情 e2e-native-mcp');
    assert.equal(await optionalText(fixture.host.codex.file), before);
    assert.ok((await extensionRecords('definitions')).some((item) => item.name === 'e2e-native-mcp'));
  });

  it('stores MCP credentials in the Windows E2E namespace and deploys/restores only confirmed changes', async () => {
    const name = 'e2e-secret-mcp';
    const token = `e2e-secret-${randomUUID()}`;
    const previous = await extensionRecords('definitions');
    await secretDraft(name, token);
    await closeExtensionDialog('添加 MCP');
    assert.deepEqual(await extensionRecords('definitions'), previous);
    const draft = await secretDraft(name, token);
    await clickButton('保存到扩展库', draft);
    const detail = await extensionDialog(`扩展详情 ${name}`);
    assert.ok(!(await detail.getText()).includes(token));
    await closeExtensionDialog(`扩展详情 ${name}`);
    const definition = (await extensionRecords('definitions')).find((item) => item.name === name);
    assert.ok(definition);
    const value = definition.payload.env.ASB_E2E_TOKEN;
    assert.equal(value.mode, 'secretRef');
    assert.match(value.reference, /^secret-[a-f0-9]{32}$/);
    assert.ok(!JSON.stringify(definition).includes(token));
    await writeFile(path.join(sandbox.artifacts, 'credential-references.json'), JSON.stringify([value.reference]));
    const before = await optionalText(fixture.host.codex.file);
    const preview = await toggleDeployment(name, 'codex');
    assert.ok(!(await preview.getText()).includes(token));
    await finishPlan(false);
    assert.equal(await optionalText(fixture.host.codex.file), before);
    await toggleDeployment(name, 'codex');
    await finishPlan(true);
    const installed = await optionalText(fixture.host.codex.file);
    assert.ok(installed.includes('[mcp_servers.e2e-secret-mcp]'));
    assert.ok(installed.includes(token));
    assert.ok(installed.includes('ASB_E2E_PUBLIC = "host-preserved"'));
    const operation = await assertOperationEvidence('install');
    const written = operation.completedSteps.find(({ step }) => step.kind === 'documentWritten').step;
    assert.equal(await optionalText(written.backupReference), before);
    await restoreLatest(name, false);
    assert.equal(await optionalText(fixture.host.codex.file), installed);
    await restoreLatest(name, true);
    assert.equal(await optionalText(fixture.host.codex.file), before);
    await expect((await libraryRow(name)).$('button[data-client="codex"]')).toHaveAttribute('aria-pressed', 'false');
  });
});
