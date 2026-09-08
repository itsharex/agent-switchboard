import { launcher as TauriLauncher } from '@wdio/tauri-service';
import { writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { readSandbox } from './support/ui.mjs';
import { captureProcessEvidence, redact } from './support/assertions.mjs';

const repository = fileURLToPath(new URL('../', import.meta.url));
const sandbox = readSandbox();
const executable = process.env.ASB_E2E_BINARY;
const outcomes = [];
// The launcher adds its matched Edge driver to PATH after reading this config.
// Keep the guarded desktop environment without replacing that updated path.
const desktopEnv = Object.fromEntries(Object.entries(sandbox.env).filter(([key]) => key.toUpperCase() !== 'PATH'));

export const config = {
  runner: 'local',
  specs: [process.env.ASB_E2E_SPEC],
  maxInstances: 1,
  hostname: '127.0.0.1',
  port: 4444,
  path: '/',
  logLevel: 'warn',
  outputDir: path.join(sandbox.artifacts, 'wdio'),
  framework: 'mocha',
  reporters: ['spec'],
  mochaOpts: { ui: 'bdd', timeout: 120000 },
  waitforTimeout: 15000,
  connectionRetryTimeout: 90000,
  connectionRetryCount: 0,
  specFileRetries: 0,
  // Use only the package's external-driver lifecycle. The worker service adds
  // console/mocking scripts; a black-box run needs no renderer injection.
  services: [[TauriLauncher, {
    driverProvider: 'external',
    tauriDriverPath: path.join(repository, 'target', 'e2e-tools', 'bin', 'tauri-driver.exe'),
    autoInstallTauriDriver: false,
    autoDownloadEdgeDriver: true,
    captureBackendLogs: true,
    captureFrontendLogs: false,
    env: desktopEnv,
  }]],
  capabilities: [{ 'tauri:options': { application: executable } }],
  async before() {
    await browser.setTimeout({ implicit: 0, pageLoad: 30000, script: 15000 });
    const handles = await browser.getWindowHandles();
    for (const handle of handles) {
      await browser.switchToWindow(handle);
      if (await $('nav[aria-label="主导航"]').isExisting()) break;
    }
    await $('nav[aria-label="主导航"]').waitForDisplayed({ timeout: 30000 });
    const evidence = await browser.execute(() => ({
      url: location.href,
      tauriIpc: typeof window.__TAURI_INTERNALS__?.invoke === 'function',
      mockRuntime: typeof window.wdioTauri !== 'undefined' || typeof window.__wdio_mocks__ !== 'undefined',
      developmentBadge: Boolean(document.querySelector('.asb-web-development-badge')),
    }));
    if (!evidence.tauriIpc || evidence.mockRuntime || evidence.developmentBadge || /127\.0\.0\.1:1420/.test(evidence.url)) {
      throw new Error(`The session is not a real production desktop WebView: ${JSON.stringify(evidence)}`);
    }
    await writeFile(path.join(sandbox.artifacts, 'webview-session.json'), JSON.stringify(evidence, null, 2));
    await captureProcessEvidence(sandbox, executable);
  },
  async afterTest(test, _context, result) {
    const slug = `${String(outcomes.length + 1).padStart(2, '0')}-${test.title.replace(/[^a-zA-Z0-9\u4e00-\u9fa5_-]/g, '-').slice(0, 85)}`;
    outcomes.push({ title: test.title, suite: test.parent, passed: result.passed, duration: result.duration, error: result.error ? redact(result.error.message) : null });
    await browser.saveScreenshot(path.join(sandbox.artifacts, `${slug}.png`));
    if (!result.passed) await writeFile(path.join(sandbox.artifacts, `${slug}.html`), redact(await browser.getPageSource()));
    await writeFile(path.join(sandbox.artifacts, 'results.json'), `${JSON.stringify(outcomes, null, 2)}\n`);
  },
};
