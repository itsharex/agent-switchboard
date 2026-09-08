import assert from 'node:assert/strict';
import { writeFile } from 'node:fs/promises';
import {
  artifactPath, clickButton, fill, navigate, openDiagnosticsSection,
  openExtensionSection, openSettingsSection, readSandbox, visibleElement, waitForText,
} from '../support/ui.mjs';
import { clientFile, clientSnapshot, createProvider, jsonFile, openProviderImport, providerRow, radio, selectClient } from '../support/provider-ui.mjs';
import path from 'node:path';

const destinations = ['供应商', '扩展', '会话', '用量', '设置'];
let profile;

async function assertLayout(label) {
  const result = await browser.execute(() => {
    const root = document.querySelector('.asb-shell');
    const viewport = document.documentElement.clientWidth;
    const outside = [...document.querySelectorAll('nav button, main button, main input, main textarea')]
      .filter((element) => element.checkVisibility())
      .filter((element) => {
        const rect = element.getBoundingClientRect();
        return rect.left < -1 || rect.right > viewport + 1;
      }).map((element) => element.getAttribute('aria-label') || element.textContent.trim());
    return { width: viewport, scrollWidth: root.scrollWidth, outside };
  });
  assert.deepEqual(result.outside, [], `${label}: clipped controls`);
  assert(result.scrollWidth <= result.width + 1, `${label}: horizontal overflow`);
  return { label, ...result };
}

async function verifyDestinations() {
  assert.deepEqual(await $$('nav[aria-label="主导航"] button').map((button) => button.getText()), destinations);
  profile = await createProvider({ app: 'codex', name: 'E2E Navigation',
    apiKey: 'e2e-navigation-key', baseUrl: `${process.env.ASB_E2E_UPSTREAM_URL}/v1`, model: 'e2e-model' });
  const layouts = [];
  for (const width of [1180, 940]) {
    await browser.setWindowSize(width, 780);
    for (const theme of ['浅色', '深色']) {
      await openSettingsSection('应用偏好');
      await radio('界面主题', theme);
      for (const page of destinations) {
        await navigate(page);
        layouts.push(await assertLayout(`${width}-${theme}-${page}`));
      }
      await openSettingsSection('客户端偏好');
      await waitForText('批准策略');
      layouts.push(await assertLayout(`${width}-${theme}-客户端偏好`));
      await browser.saveScreenshot(artifactPath(`preferences-${width}-${theme}.png`));
      for (const section of ['配置与环境', '本机网关', '运行日志']) {
        await openDiagnosticsSection(section);
        layouts.push(await assertLayout(`${width}-${theme}-${section}`));
      }
      await navigate('供应商');
      await browser.saveScreenshot(artifactPath(`providers-${width}-${theme}.png`));
    }
  }
  await writeFile(artifactPath('navigation-layouts.json'), JSON.stringify(layouts, null, 2));
}

async function preserveProviderDraft() {
  await browser.setWindowSize(1180, 780);
  await openSettingsSection('应用偏好');
  await radio('界面主题', '浅色');
  await navigate('供应商');
  const before = await clientSnapshot('codex');
  await providerRow(profile.name);
  await clickButton(`编辑 ${profile.name}`);
  await browser.saveScreenshot(artifactPath('provider-editor.png'));
  await fill('名称', 'E2E unsaved navigation draft');
  const disclosure = await $('//details[summary/span[normalize-space(.)="Responses 能力"]]');
  const summary = await disclosure.$('summary');
  await summary.scrollIntoView();
  await summary.click();
  await browser.keys('Enter');
  assert.equal(await disclosure.getAttribute('open'), null, 'Native Enter closes the focused disclosure');
  await browser.keys(' ');
  assert.notEqual(await disclosure.getAttribute('open'), null, 'Native Space opens the focused disclosure');
  await clickButton('配置运行参数');
  await browser.saveScreenshot(artifactPath('provider-parameters.png'));
  await openSettingsSection('客户端偏好');
  await selectClient('claude', '客户端偏好客户端');
  await navigate('供应商');
  await waitForText('运行参数');
  await clickButton('返回供应商编辑');
  await expect(await visibleElement('input[value="E2E unsaved navigation draft"]')).toBeDisplayed();
  await clickButton('取消');
  assert.deepEqual(await clientSnapshot('codex'), before);
  await selectClient('codex', '供应商客户端');
  await providerRow(profile.name);
  await clickButton(`预览 ${profile.name} 变更`);
  const preview = await $('section[aria-label="变更预览"]');
  await preview.waitForDisplayed();
  await preview.scrollIntoView({ block: 'start' });
  await browser.saveScreenshot(artifactPath('switch-preview.png'));
  await clickButton('取消', preview);
}

async function preserveInstructionsAndQuotaLocation() {
  await openExtensionSection('全局指令');
  await selectClient('codex', '全局指令客户端');
  await fill('AGENTS.md 内容', 'Unsaved navigation instruction');
  await navigate('用量');
  await clickButton('额度与重置');
  await waitForText('Codex 官方额度重置');
  await navigate('扩展');
  await expect($('textarea[aria-label="AGENTS.md 内容"]')).toHaveValue('Unsaved navigation instruction');
  await clickButton('放弃草稿');
  await navigate('用量');
  await expect($('.asb-workspace-tabs [role="tab"][aria-selected="true"]')).toHaveText('额度与重置');
}

async function importIntoSelectedClient() {
  const fixture = await jsonFile(path.join(readSandbox().fixtures, 'manifest.json'));
  await writeFile(clientFile('claude'), fixture.discovery.claudeContent);
  const before = await clientSnapshot('claude');
  await openProviderImport('本机配置');
  await selectClient('claude', '导入客户端');
  const panel = await $('section[aria-label="从本机配置导入"]');
  await clickButton('扫描配置', panel);
  await clickButton('导入供应商', panel);
  await providerRow(fixture.discovery.claudeName);
  await expect($('[role="radiogroup"][aria-label="供应商客户端"] label.is-active')).toHaveText('Claude');
  assert.deepEqual(await clientSnapshot('claude'), before);
}

describe('Five workspaces through the real desktop UI', () => {
  it('exposes all destinations and maintenance sections at normal and minimum desktop widths in both themes', async function () {
    this.timeout(240000);
    await verifyDestinations();
  });
  it('keeps the provider draft and runtime subpage across client changes and supports native disclosure keys', preserveProviderDraft);
  it('keeps global instruction drafts and the selected usage category across workspaces', preserveInstructionsAndQuotaLocation);
  it('imports the selected client and returns to its provider without applying a switch', importIntoSelectedClient);
});
