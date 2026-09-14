import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFile, readdir, writeFile } from 'node:fs/promises';
import path from 'node:path';
import {
  clickButton, fill, navigate, readSandbox, restartDesktop, visibleElement, waitForText, xpathText,
} from '../support/ui.mjs';
import { appendUsageFixture } from '../support/fixture-sessions.mjs';
import {
  jsonFile, openProviderImport, providerFiles, providerRow, radio, readOptional, selectClient, waitForFile,
} from '../support/provider-ui.mjs';

let sandbox;
let fixture;

async function sessionRow(title) {
  return visibleElement(`.//section[@aria-label='会话列表']//button[.//span[normalize-space(.)=${xpathText(title)}]]`);
}

async function assertSessionSourcesUnchanged() {
  for (const session of fixture.sessions) assert.equal(await readFile(session.file, 'utf8'), session.content);
}

async function usageRows() {
  const table = await visibleElement('table[aria-label="模型消耗"]');
  const result = [];
  for (const row of await table.$$('tbody tr')) {
    const cells = await row.$$('td');
    result.push({
      client: await cells[0].getText(), model: await cells[1].getText(),
      input: Number((await cells[2].getText()).replaceAll(',', '')),
      cache: Number((await cells[3].getText()).replaceAll(',', '')),
      output: Number((await cells[4].getText()).replaceAll(',', '')),
      total: Number((await cells[5].getText()).replaceAll(',', '')),
      sessions: Number((await cells[6].getText()).replaceAll(',', '')),
    });
  }
  return result;
}

async function assertUsageTotal(total) {
  await browser.waitUntil(async () => (await usageRows()).reduce((sum, row) => sum + row.total, 0) === total, {
    timeout: 15000, timeoutMsg: `Visible usage rows did not total ${total} tokens`,
  });
}

async function scanLocalConfiguration() {
  const section = await $('section[aria-label="从本机配置导入"]');
  const button = await visibleElement('.//button[normalize-space(.)="扫描配置" or normalize-space(.)="刷新配置"]', section);
  await button.click();
  await button.waitForEnabled();
  await (await section.$('article[aria-label="Claude 扫描结果"]')).waitForDisplayed();
  return section;
}

async function assertCcSwitchUntouched() {
  const bytes = await readFile(fixture.ccSwitch.file);
  assert.equal(createHash('sha256').update(bytes).digest('hex'), fixture.ccSwitch.sha256);
  assert.deepEqual(await readdir(path.dirname(fixture.ccSwitch.file)), ['cc-switch.db']);
}

before(async () => {
  sandbox = readSandbox();
  fixture = await jsonFile(path.join(sandbox.fixtures, 'manifest.json'));
});

describe('Real desktop sessions and CLI resume', () => {
  it('scans and filters native sessions, searches IDs, and renders the actual transcript', async () => {
    await navigate('会话');
    await clickButton('刷新会话');
    await browser.waitUntil(async () => (await $$('.asb-session-item')).length === fixture.sessions.length);
    await radio('会话客户端筛选', 'Codex');
    await expect(await $$('.asb-session-item')).toBeElementsArrayOfSize(3);
    await radio('会话客户端筛选', 'Claude');
    await expect(await $$('.asb-session-item')).toBeElementsArrayOfSize(2);
    await radio('会话客户端筛选', '全部');
    const target = fixture.sessions.find((item) => item.id === 'e2e-codex-today');
    await fill('搜索会话', target.id);
    await expect(await $$('.asb-session-item')).toBeElementsArrayOfSize(1);
    await (await sessionRow(target.title)).click();
    const detail = await $('section[aria-label="会话详情"]');
    await expect(detail).toHaveText(expect.stringContaining(`codex resume ${target.id}`));
    await waitForText(`${target.title} deterministic reply`);
    await fill('搜索会话', 'e2e-no-such-session');
    await waitForText('未找到匹配的 Codex 或 Claude Code 会话');
    await fill('搜索会话', '');
    await assertSessionSourcesUnchanged();
  });

  it('resumes both clients through the fixed terminal command with isolated CLI recorders', async () => {
    for (const app of ['codex', 'claude']) {
      const target = fixture.sessions.find((item) => item.id === `e2e-${app}-today`);
      await (await sessionRow(target.title)).click();
      await clickButton('在终端中恢复', await $('section[aria-label="会话详情"]'));
      await waitForText('已在新终端窗口中恢复会话');
      await waitForFile(fixture.resumeLog, (text) => text !== null && text.includes(target.id));
      const entries = (await readFile(fixture.resumeLog, 'utf8')).trim().split('\n').map((line) => JSON.parse(line));
      const entry = entries.find((item) => item.argv.includes(target.id));
      assert.deepEqual(entry.argv, [app, app === 'codex' ? 'resume' : '--resume', target.id]);
      assert.equal(path.resolve(entry.cwd).toLowerCase(), path.resolve(target.projectDir).toLowerCase());
    }
    await assertSessionSourcesUnchanged();
  });

  it('deletes a disposable session only through the confirmation sheet and removes its record file', async () => {
    const id = 'e2e-codex-delete-me';
    const title = 'E2E Codex delete me';
    const file = path.join(sandbox.home, '.codex', 'sessions', `${id}.jsonl`);
    await writeFile(file, `${JSON.stringify({
      type: 'session_meta', timestamp: new Date().toISOString(), payload: { id, cwd: sandbox.fixtures }, customTitle: title,
    })}\n`);

    await navigate('会话');
    await clickButton('刷新会话');
    await (await sessionRow(title)).click();
    const detail = await $('section[aria-label="会话详情"]');
    await clickButton('删除会话', detail);
    await clickButton('确认删除');

    await waitForText('已删除会话');
    await browser.waitUntil(async () => {
      const rows = await $$('.asb-session-item');
      const texts = await Promise.all(rows.map((row) => row.getText()));
      return !texts.some((text) => text.includes(title));
    }, { timeout: 15000, timeoutMsg: 'deleted session is still listed' });
    await waitForText('选择一条会话即可查看内容并复制恢复命令。');
    assert.equal(await readOptional(file), null);
  });

});

describe('Real desktop usage and persisted snapshots', () => {
  it('totals real JSONL tokens for all four calendar ranges and persists matching snapshots', async () => {
    await navigate('用量');
    await clickButton('消耗统计', await $('[role="tablist"][aria-label="用量分类"]'));
    await assertUsageTotal(fixture.totals.today);
    const today = await usageRows();
    assert.deepEqual(today.find((row) => row.model === 'e2e-gpt-today'), { client: 'Codex', model: 'e2e-gpt-today', input: 100, cache: 20, output: 30, total: 150, sessions: 1 });
    assert.deepEqual(today.find((row) => row.model === 'e2e-claude-today'), { client: 'Claude', model: 'e2e-claude-today', input: 40, cache: 15, output: 15, total: 70, sessions: 1 });
    for (const [range, label] of [['last7Days', '近 7 天'], ['last30Days', '近 30 天'], ['all', '全部']]) {
      await radio('模型消耗时间范围', label);
      await assertUsageTotal(fixture.totals[range]);
    }
    const cache = await jsonFile(path.join(sandbox.state, 'model-usage-cache.json'));
    assert.equal(cache.entries.length, 4);
    for (const entry of cache.entries) {
      assert.equal(entry.report.groups.reduce((sum, group) => sum + group.totalTokens, 0), fixture.totals[entry.range]);
    }
    await assertSessionSourcesUnchanged();
  });

  it('reuses cached reports until refresh and keeps the last selected range visible', async () => {
    await radio('模型消耗时间范围', '今日');
    await assertUsageTotal(fixture.totals.today);
    const cachePath = path.join(sandbox.state, 'model-usage-cache.json');
    const before = await readFile(cachePath, 'utf8');
    const appended = await appendUsageFixture(sandbox);
    await navigate('会话');
    await navigate('用量');
    await assertUsageTotal(fixture.totals.today);
    assert.equal(await readFile(cachePath, 'utf8'), before);
    await clickButton('刷新', await $('section[aria-label="模型消耗"]'));
    await assertUsageTotal(fixture.totals.today + appended.total);
    assert.ok((await usageRows()).some((row) => row.model === 'e2e-refresh-model' && row.total === 20));
    await radio('模型消耗时间范围', '近 30 天');
    await radio('模型消耗时间范围', '全部');
    await radio('模型消耗时间范围', '今日');
    await assertUsageTotal(fixture.totals.today + appended.total);
    await expect(await $('.//div[@aria-label="模型消耗时间范围"]//label[normalize-space(.)="今日"]/input')).toBeSelected();
    const updated = await jsonFile(cachePath);
    assert.equal(updated.entries.find((entry) => entry.range === 'today').report.groups.reduce((sum, row) => sum + row.totalTokens, 0), 240);
    assert.equal(await readFile(appended.file, 'utf8'), appended.content);
  });

});

describe('Real desktop host discovery and import', () => {
  it('scans host configuration read-only and restores its cached result after restarting the real application', async () => {
    await writeFile(fixture.host.claude.file, fixture.discovery.claudeContent);
    await openProviderImport('本机配置');
    await selectClient('claude', '导入客户端');
    await scanLocalConfiguration();
    const card = await $('article[aria-label="Claude 扫描结果"]');
    await expect(card).toHaveText(expect.stringContaining(fixture.discovery.model));
    const cachePath = path.join(sandbox.state, 'discovery-cache.json');
    const cached = await readFile(cachePath, 'utf8');
    assert.ok(!cached.includes(fixture.discovery.apiKey));
    assert.equal(await readFile(fixture.host.claude.file, 'utf8'), fixture.discovery.claudeContent);
    const external = fixture.discovery.claudeContent.replace(fixture.discovery.model, 'e2e-discovery-external');
    await writeFile(fixture.host.claude.file, external);
    await restartDesktop();
    await openProviderImport('本机配置');
    await selectClient('claude', '导入客户端');
    await expect(await $('article[aria-label="Claude 扫描结果"]')).toHaveText(expect.stringContaining(fixture.discovery.model));
    assert.equal(await readFile(cachePath, 'utf8'), cached);
    await scanLocalConfiguration();
    await expect(await $('article[aria-label="Claude 扫描结果"]')).toHaveText(expect.stringContaining('e2e-discovery-external'));
    assert.equal(await readFile(fixture.host.claude.file, 'utf8'), external);
    await writeFile(fixture.host.claude.file, fixture.discovery.claudeContent);
    await scanLocalConfiguration();
  });

  it('shows malformed external configuration and imports a valid native provider through the UI', async () => {
    const before = await providerFiles('claude');
    await writeFile(fixture.host.claude.file, '{ broken external JSON');
    await scanLocalConfiguration();
    const card = await $('article[aria-label="Claude 扫描结果"]');
    await expect(card).toHaveText(expect.stringContaining('语法错误'));
    await expect(card.$('button=导入供应商')).not.toExist();
    assert.deepEqual(await providerFiles('claude'), before);
    assert.equal(await readFile(fixture.host.claude.file, 'utf8'), '{ broken external JSON');
    await writeFile(fixture.host.claude.file, fixture.discovery.claudeContent);
    await scanLocalConfiguration();
    await clickButton('导入供应商', await $('article[aria-label="Claude 扫描结果"]'));
    await waitForText(fixture.discovery.claudeName);
    const imported = (await providerFiles('claude')).find((item) => item.name === fixture.discovery.claudeName);
    assert.ok(imported);
    assert.equal(imported.routeMode, 'custom');
    assert.equal(imported.upstreamProtocol, 'anthropicMessages');
    assert.equal(imported.baseUrl, fixture.discovery.baseUrl);
    assert.equal(imported.model, fixture.discovery.model);
    assert.equal(imported.apiKey, fixture.discovery.apiKey);
    assert.ok(imported.parameters && typeof imported.parameters === 'object');
    assert.equal(await readFile(fixture.host.claude.file, 'utf8'), fixture.discovery.claudeContent);
    assert.equal(await readOptional(fixture.host.codex.file), fixture.host.codex.content);
  });

});

describe('Real desktop read-only CC Switch source', () => {
  it('scans CC Switch without writing its database and imports only the selected supported provider', async () => {
    await openProviderImport('CC Switch');
    let section = await $('section[aria-label="从 CC Switch 导入"]');
    await clickButton('扫描 CC Switch（只读）', section);
    await waitForText(fixture.ccSwitch.name);
    await waitForText('E2E unsupported client');
    const table = await section.$('table[aria-label="CC Switch 扫描结果"]');
    assert.ok(!(await table.getText()).includes(fixture.ccSwitch.credential));
    const label = await visibleElement(`.//label[.//span[normalize-space(.)=${xpathText(fixture.ccSwitch.name)}]]`, section);
    await expect(label.$('input')).toBeSelected();
    await label.click();
    await expect(section.$('button=导入所选 0 项')).toBeDisabled();
    await assertCcSwitchUntouched();
    await label.click();
    await clickButton('导入所选 1 项', section);
    await providerRow(fixture.ccSwitch.name);
    const imported = (await providerFiles('claude')).find((item) => item.name === fixture.ccSwitch.name);
    assert.ok(imported);
    assert.equal(imported.model, fixture.ccSwitch.model);
    assert.equal(imported.apiKey, fixture.ccSwitch.credential);
    assert.equal(imported.upstreamProtocol, 'anthropicMessages');
    await assertCcSwitchUntouched();
    assert.equal(await readFile(fixture.host.claude.file, 'utf8'), fixture.discovery.claudeContent);
    await openProviderImport('CC Switch');
    section = await $('section[aria-label="从 CC Switch 导入"]');
    await waitForText('已导入 1 项');
    await clickButton('扫描 CC Switch（只读）', section);
    await waitForText('已存在相同档案，导入将跳过');
    assert.equal((await providerFiles('claude')).filter((item) => item.name === fixture.ccSwitch.name).length, 1);
    await clickButton('返回供应商');
  });
});
