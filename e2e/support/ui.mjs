import assert from 'node:assert/strict';
import path from 'node:path';

export function readSandbox() {
  assert.ok(process.env.ASB_E2E_SANDBOX, 'Use run-desktop-e2e.mjs to create an isolated desktop session');
  return JSON.parse(process.env.ASB_E2E_SANDBOX);
}

export function xpathText(value) {
  if (!value.includes("'")) return `'${value}'`;
  return `concat(${value.split("'").map((part) => `'${part}'`).join(', "\'", ')})`;
}

export async function visibleElement(selector, scope = browser) {
  let match;
  await browser.waitUntil(async () => {
    for (const element of await scope.$$(selector)) {
      if (await element.isDisplayed()) { match = element; return true; }
    }
    return false;
  }, { timeout: 15000, timeoutMsg: `No visible UI element: ${selector}` });
  return match;
}

export async function clickButton(text, scope = browser) {
  const value = xpathText(text);
  const element = await visibleElement(`.//button[normalize-space(.)=${value} or normalize-space(text())=${value} or @aria-label=${value} or @title=${value}]`, scope);
  await element.waitForEnabled({ timeout: 15000 });
  await element.scrollIntoView({ block: 'center' });
  await element.click();
  return element;
}

export async function fill(label, value, scope = browser) {
  const text = xpathText(label);
  const selector = `.//input[@aria-label=${text} or @placeholder=${text}] | .//textarea[@aria-label=${text} or @placeholder=${text}] | .//label[.//span[normalize-space(.)=${text}] or normalize-space(text())=${text}]//input | .//label[.//span[normalize-space(.)=${text}] or normalize-space(text())=${text}]//textarea`;
  const field = await visibleElement(selector, scope);
  await field.scrollIntoView({ block: 'center' });
  await field.setValue(String(value));
  return field;
}

export async function selectOption(label, optionText, scope = browser) {
  const text = xpathText(label);
  const control = await visibleElement(`.//select[@aria-label=${text}] | .//label[.//span[normalize-space(.)=${text}]]//select | .//*[@role='combobox' and @aria-label=${text}] | .//label[.//span[normalize-space(.)=${text}]]//*[@role='combobox']`, scope);
  await control.scrollIntoView({ block: 'center' });
  if (await control.getTagName() === 'select') return control.selectByVisibleText(optionText);
  await control.click();
  const option = await visibleElement(`.//*[@role='option' and normalize-space(.)=${xpathText(optionText)}]`);
  await option.click();
}

export async function navigate(label) {
  assert(['供应商', '扩展', '会话', '用量', '设置'].includes(label), `Unknown main workspace: ${label}`);
  const nav = await visibleElement('nav[aria-label="主导航"]');
  const button = await clickButton(label, nav);
  await expect(button).toHaveAttribute('aria-current', 'page');
  await browser.waitUntil(async () => !(await $('[aria-label="处理中"]').isDisplayed()), { timeout: 15000 });
}

export async function openSettingsSection(label) {
  assert(['应用偏好', '客户端偏好', '备份与恢复', '诊断', '关于与更新'].includes(label), `Unknown settings category: ${label}`);
  await navigate('设置');
  const navigation = await visibleElement('nav[aria-label="设置分类"]');
  const button = await clickButton(label, navigation);
  await expect(button).toHaveAttribute('aria-current', 'page');
}

export async function openDiagnosticsSection(label) {
  assert(['配置与环境', '本机网关', '运行日志'].includes(label), `Unknown diagnostics section: ${label}`);
  await openSettingsSection('诊断');
  const controls = await visibleElement('[role="group"][aria-label="诊断内容"]');
  const button = await clickButton(label, controls);
  await expect(button).toHaveAttribute('aria-pressed', 'true');
}

export async function openExtensionSection(label) {
  assert(['Skills', 'MCP', '全局指令'].includes(label), `Unknown extension section: ${label}`);
  await navigate('扩展');
  const tabs = await visibleElement('[role="tablist"][aria-label="扩展内容"]');
  const tab = await clickButton(label, tabs);
  await expect(tab).toHaveAttribute('aria-selected', 'true');
}

export async function waitForText(text) {
  return visibleElement(`.//*[contains(normalize-space(.), ${xpathText(text)}) and not(.//*[contains(normalize-space(.), ${xpathText(text)})])]`);
}

export async function restartDesktop() {
  await browser.reloadSession();
  await $('nav[aria-label="主导航"]').waitForDisplayed({ timeout: 30000 });
}

export function artifactPath(name) {
  assert.equal(path.basename(name), name);
  return path.join(readSandbox().artifacts, name);
}
