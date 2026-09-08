import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import path from 'node:path';
import { navigate, readSandbox } from '../support/ui.mjs';
import { refreshConfiguration } from '../support/provider-ui.mjs';

const pages = {
  '供应商': '供应商', '扩展': '扩展', '会话': '会话', '用量': '用量', '设置': '设置',
};

describe('Actual Windows desktop navigation', () => {
  for (const [page, heading] of Object.entries(pages)) {
    it(`opens ${page} through the visible navigation`, async () => {
      await navigate(page);
      const text = await $('body').getText();
      assert(text.includes(heading), `${page} page did not render its content`);
      assert(!text.includes('浏览器开发 · 本机后端'));
      const nav = await $('nav[aria-label="主导航"]');
      const labels = await nav.$$('button').map((button) => button.getText());
      assert.deepEqual(labels.map((label) => label.trim()), Object.keys(pages));
      const active = await nav.$('button[aria-current="page"]').getText();
      assert.equal(active.trim(), page);
    });
  }

  it('refreshes native config status and displays only isolated runtime paths', async () => {
    const sandbox = readSandbox();
    await refreshConfiguration();
    await browser.waitUntil(async () => (await $('body').getText()).includes('配置正常'));
    const body = await $('body').getText();
    assert(body.includes(sandbox.codex), 'Configuration diagnostics must show the actual isolated Codex config path');
    assert(body.includes(sandbox.claude), 'Configuration diagnostics must show the actual isolated Claude config path');
    assert((await readFile(path.join(sandbox.codex, 'config.toml'), 'utf8')).includes('host-preserved'));
    assert.equal(JSON.parse(await readFile(path.join(sandbox.claude, 'settings.json'), 'utf8')).env.ASB_E2E_HOST, 'preserve-me');
  });
});
