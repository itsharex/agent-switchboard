import assert from 'node:assert/strict';
import { randomUUID } from 'node:crypto';
import { readFile, unlink, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { clickButton, fill, openExtensionSection, readSandbox, visibleElement, waitForText } from '../support/ui.mjs';
import {
  assertOperationEvidence, closeExtensionDialog, confirmSensitiveWrite, discoveryRow, extensionDialog,
  extensionRecords, importNativeSelection, libraryRow, openDiscovery, optionalText, restoreLatest, scanExtensions,
  toggleDeployment, waitForToast,
} from '../support/extensions-ui.mjs';

let sandbox;
let fixture;

async function secretDraft(name, token) {
  await clickButton('添加 MCP');
  const dialog = await extensionDialog('添加 MCP');
  await fill('服务名称', name, dialog);
  await fill('MCP JSON 配置', JSON.stringify({
    command: 'cmd.exe', args: ['/d', '/c', 'echo e2e-secret-mcp'],
    env: { ASB_E2E_TOKEN: { mode: 'secret', value: token } },
  }), dialog);
  await (await dialog.$('button[aria-label="保存后启用 Claude"]')).click();
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

  it('cancels then imports a selected native Skill without changing its bytes', async () => {
    await openDiscovery();
    await closeExtensionDialog('从本机发现');
    assert.deepEqual(await extensionRecords('definitions'), []);
    assert.equal(await optionalText(fixture.skills.guide.file), fixture.skills.guide.content);
    await openDiscovery();
    await importNativeSelection(fixture.skills.name);
    assert.equal(await optionalText(fixture.skills.manifest.file), fixture.skills.manifest.content);
    assert.equal(await optionalText(fixture.skills.guide.file), fixture.skills.guide.content);
    assert.ok((await extensionRecords('definitions')).some((item) => item.name === fixture.skills.name));
    assert.equal((await extensionRecords('bindings')).length, 1);
    assert.equal((await extensionRecords('baselines')).length, 1);
  });

});

describe('Real desktop Skill deployment and restoration', () => {
  it('deploys actual Skill files to Claude on one toggle and restores a first install to file absence', async () => {
    // Skill deploys write no credential data: one click commits the whole
    // transaction with no confirmation step in between.
    const sheet = await toggleDeployment(fixture.skills.name, 'claude');
    assert.equal(sheet, null);
    const manifest = path.join(fixture.skills.claudeTarget, 'SKILL.md');
    const guide = path.join(fixture.skills.claudeTarget, 'references', 'guide.md');
    await browser.waitUntil(async () => (await optionalText(manifest)) === fixture.skills.manifest.content);
    assert.equal(await optionalText(guide), fixture.skills.guide.content);
    const operation = await assertOperationEvidence('install');
    assert.ok(operation.completedSteps.some(({ step }) => step.kind === 'directoryDeployed' && step.originalDigest === null));
    await restoreLatest(fixture.skills.name);
    assert.equal(await optionalText(manifest), null);
    assert.equal(await optionalText(guide), null);
    assert.equal(await optionalText(fixture.skills.manifest.file), fixture.skills.manifest.content);
    await assertOperationEvidence('restore');
  });

});

describe('Real desktop missing Skill repair', () => {
  it('restores the missing Skill file with one repair click', async () => {
    assert.equal(await readFile(fixture.skills.guide.file, 'utf8'), fixture.skills.guide.content);
    await unlink(fixture.skills.guide.file);
    const dialog = await openDiscovery();
    await scanExtensions();
    const repair = await visibleElement('.//button[@aria-label="修复这 1 项可修复警告"]', dialog);
    await repair.click();
    // The repair applies immediately and the follow-up scan clears the
    // now-fixed warning.
    await browser.waitUntil(async () => await optionalText(fixture.skills.guide.file) === fixture.skills.guide.content);
    await expect(dialog.$('button[aria-label="修复这 1 项可修复警告"]')).not.toExist();
    await assertOperationEvidence('repair');
    await closeExtensionDialog('从本机发现');
  });

});

describe('Real desktop MCP and system credentials', () => {
  it('imports a native MCP entry while preserving its host document', async () => {
    await clickButton('MCP', await $('section[aria-label="扩展"]'));
    await openDiscovery();
    const before = await optionalText(fixture.host.codex.file);
    await importNativeSelection('e2e-native-mcp');
    assert.equal(await optionalText(fixture.host.codex.file), before);
    assert.ok((await extensionRecords('definitions')).some((item) => item.name === 'e2e-native-mcp'));
  });

  it('stores MCP credentials in the Windows E2E namespace and confirms the one sensitive write', async () => {
    const name = 'e2e-secret-mcp';
    const token = `e2e-secret-${randomUUID()}`;
    const previous = await extensionRecords('definitions');
    await secretDraft(name, token);
    await closeExtensionDialog('添加 MCP');
    assert.deepEqual(await extensionRecords('definitions'), previous);
    const before = await optionalText(fixture.host.codex.file);

    // Saving deploys immediately; installing a server whose env holds a
    // credential is the one write that still stops for confirmation.
    const draft = await secretDraft(name, token);
    await clickButton('保存并启用', draft);
    let sheet = await extensionDialog('确认安装（写入敏感数据）');
    assert.ok(!(await sheet.getText()).includes(token));
    await confirmSensitiveWrite(sheet, false);
    await closeExtensionDialog(`扩展详情 ${name}`);
    assert.equal(await optionalText(fixture.host.codex.file), before);
    assert.ok((await extensionRecords('definitions')).some((item) => item.name === name));

    // An external edit made while the confirmation is pending is rejected.
    sheet = await toggleDeployment(name, 'codex');
    const foreign = `${before}\n# external edit while the write was pending\n`;
    await writeFile(fixture.host.codex.file, foreign);
    await confirmSensitiveWrite(sheet, true);
    await waitForToast(/变更未能应用|应用失败/);
    assert.equal(await optionalText(fixture.host.codex.file), foreign);
    await writeFile(fixture.host.codex.file, before);

    // With the document back to its prepared state the same confirmation
    // commits the transaction.
    sheet = await toggleDeployment(name, 'codex');
    await confirmSensitiveWrite(sheet, true);
    const installed = await optionalText(fixture.host.codex.file);
    assert.ok(installed.includes('[mcp_servers.e2e-secret-mcp]'));
    assert.ok(installed.includes(token));
    assert.ok(installed.includes('ASB_E2E_PUBLIC = "host-preserved"'));
    const operation = await assertOperationEvidence('install');
    const written = operation.completedSteps.find(({ step }) => step.kind === 'documentWritten').step;
    assert.equal(await optionalText(written.backupReference), before);
    const definition = (await extensionRecords('definitions')).find((item) => item.name === name);
    assert.ok(definition);
    const value = definition.payload.env.ASB_E2E_TOKEN;
    assert.equal(value.mode, 'secretRef');
    assert.match(value.reference, /^secret-[a-f0-9]{32}$/);
    assert.ok(!JSON.stringify(definition).includes(token));
    await writeFile(path.join(sandbox.artifacts, 'credential-references.json'), JSON.stringify([value.reference]));
    await restoreLatest(name);
    assert.equal(await optionalText(fixture.host.codex.file), before);
    await expect((await libraryRow(name)).$('button[data-client="codex"]')).toHaveAttribute('aria-pressed', 'false');
  });
});
