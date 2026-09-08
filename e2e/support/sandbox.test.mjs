import assert from 'node:assert/strict';
import { execFile } from 'node:child_process';
import { randomUUID } from 'node:crypto';
import { mkdir, readFile, readdir, rm, symlink, writeFile } from 'node:fs/promises';
import path from 'node:path';
import test, { after, before } from 'node:test';
import { fileURLToPath } from 'node:url';
import { promisify } from 'node:util';
import {
  acquireRunLock, assertSandbox, cleanupSandbox, createSandbox, recordSandboxSecrets, releaseRunLock,
} from './sandbox.mjs';

const execFileAsync = promisify(execFile);
const windowsOnly = { skip: process.platform !== 'win32' };
const freshReference = () => `secret-${randomUUID().replaceAll('-', '')}`;
let runLock;

before(async () => { if (process.platform === 'win32') runLock = await acquireRunLock(); });
after(async () => { if (runLock) await releaseRunLock(runLock); });

async function nativeCommand(sandbox, script, args) {
  const systemRoot = Object.entries(sandbox.env).find(([key]) => key.toUpperCase() === 'SYSTEMROOT')[1];
  return execFileAsync(path.join(systemRoot, 'System32', 'WindowsPowerShell', 'v1.0', 'powershell.exe'), [
    '-NoLogo', '-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass', '-File',
    fileURLToPath(new URL(script, import.meta.url)), ...args,
  ], { env: sandbox.env, cwd: sandbox.root, windowsHide: true, timeout: 15_000 });
}

async function writeCredentials(sandbox, references) {
  for (const reference of references) {
    await nativeCommand(sandbox, './sandbox-credential-fixture.ps1', ['-Reference', reference]);
  }
}

async function snapshotCredentials(sandbox) {
  const result = await nativeCommand(sandbox, './windows-credentials.ps1', ['-Mode', 'Snapshot']);
  return JSON.parse(result.stdout.trim());
}

async function writeDefinition(sandbox, document) {
  const directory = path.join(sandbox.state, 'extensions', 'definitions');
  await mkdir(directory, { recursive: true });
  const file = path.join(directory, 'fixture.json');
  await writeFile(file, typeof document === 'string' ? document : JSON.stringify(document));
  return file;
}

async function cleanupAttempts(sandbox) {
  const directory = path.join(sandbox.artifacts, 'cleanup-attempts');
  const names = (await readdir(directory)).sort();
  return Promise.all(names.map(async name => JSON.parse(await readFile(path.join(directory, name, 'result.json'), 'utf8'))));
}

test('isolated environment removes inherited API keys, proxies and development dispatch', windowsOnly, async () => {
  const injected = {
    OPENAI_API_KEY: 'e2e-environment-sentinel', ANTHROPIC_AUTH_TOKEN: 'e2e-environment-sentinel',
    ASB_WEB_DEVELOPMENT: '1', HTTPS_PROXY: 'http://127.0.0.1:9',
  };
  const previous = Object.fromEntries(Object.keys(injected).map(key => [key, process.env[key]]));
  let sandbox;
  try {
    Object.assign(process.env, injected);
    sandbox = await createSandbox({ runId: `support-env-${randomUUID()}` });
    for (const key of Object.keys(injected)) assert.equal(sandbox.env[key], undefined);
    assert.equal(sandbox.env.WEBVIEW2_USER_DATA_FOLDER, sandbox.webview);
    assert.equal(sandbox.env.USERPROFILE, sandbox.home);
    const saved = await cleanupSandbox(sandbox, { success: false });
    assert.equal(saved.removed, false);
    assert.equal(JSON.parse(await readFile(path.join(sandbox.artifacts, 'credential-cleanup.json'))).verified, true);
  } finally {
    for (const [key, value] of Object.entries(previous)) {
      if (value === undefined) delete process.env[key];
      else process.env[key] = value;
    }
    if (sandbox) await cleanupSandbox(sandbox, { success: true });
  }
});

test('ownership, environment and junction guards reject escaped cleanup targets', windowsOnly, async () => {
  await assert.rejects(createSandbox({ runId: '../outside' }), /Invalid E2E run ID/);
  const sandbox = await createSandbox({ runId: `support-guards-${randomUUID()}` });
  try {
    await assert.rejects(assertSandbox({ ...sandbox, root: path.dirname(sandbox.root) }), /direct child/);
    await assert.rejects(assertSandbox({ ...sandbox, env: { ...sandbox.env, USERPROFILE: 'C:\\outside' } }), /USERPROFILE/);
    const originalHome = sandbox.home;
    const replacedHome = path.join(sandbox.root, 'original-home');
    await mkdir(replacedHome);
    await rm(originalHome, { recursive: true });
    await symlink(replacedHome, originalHome, 'junction');
    await assert.rejects(assertSandbox(sandbox), /symlink or junction/);
    await rm(originalHome);
    await mkdir(originalHome);
  } finally {
    await cleanupSandbox(sandbox, { success: true });
  }
});

test('native Credential Manager cleanup covers both recorded and orphaned E2E saves', windowsOnly, async () => {
  const sandbox = await createSandbox({ runId: `support-credentials-${randomUUID()}` });
  const references = [freshReference(), freshReference()];
  try {
    await writeCredentials(sandbox, references);
    await writeDefinition(sandbox, { definition: { bearer: { mode: 'secretRef', reference: references[0] } } });
    assert.deepEqual(await recordSandboxSecrets(sandbox), [references[0]]);
    const result = await cleanupSandbox(sandbox, { success: false });
    for (const reference of references) {
      assert.ok(result.credentials.deletedTargets.includes(`Agent Switchboard E2E.${reference}`));
    }
    assert.deepEqual(result.credentials.remainingNewTargets, []);
    const firstEvidence = await readFile(path.join(sandbox.artifacts, 'credential-cleanup.json'), 'utf8');
    await cleanupSandbox(sandbox, { success: false });
    assert.equal(await readFile(path.join(sandbox.artifacts, 'credential-cleanup.json'), 'utf8'), firstEvidence);
    assert.equal((await cleanupAttempts(sandbox)).length, 2);
  } finally {
    await cleanupSandbox(sandbox, { success: true });
  }
});

for (const [label, document, expected] of [
  ['malformed JSON', '{', /Malformed JSON/],
  ['invalid reference', { mode: 'secretRef', reference: '../not-a-reference' }, /invalid credential reference/],
  ['obsolete kind contract', { kind: 'secretRef', reference: `secret-${'a'.repeat(32)}` }, /obsolete kind/],
]) {
  test(`native cleanup still removes new credentials after ${label}`, windowsOnly, async () => {
    const sandbox = await createSandbox({ runId: `support-invalid-${randomUUID()}` });
    const reference = freshReference();
    let definition;
    try {
      await writeCredentials(sandbox, [reference]);
      definition = await writeDefinition(sandbox, document);
      await assert.rejects(cleanupSandbox(sandbox, { success: true }), error => {
        assert.ok(error instanceof AggregateError);
        assert.ok(error.errors.some(failure => failure.stage === 'reference-scan' && expected.test(failure.message)));
        assert.equal(error.cleanupResult.removed, false);
        assert.equal(error.cleanupResult.credentials.verified, true);
        assert.ok(error.cleanupResult.credentials.deletedTargets.includes(`Agent Switchboard E2E.${reference}`));
        return true;
      });
      assert.ok(!(await snapshotCredentials(sandbox)).includes(`Agent Switchboard E2E.${reference}`));
      const attempts = await cleanupAttempts(sandbox);
      assert.equal(attempts[0].outcome, 'retained-after-errors');
      assert.equal(attempts[0].credentials.verified, true);
    } finally {
      if (definition) await rm(definition);
      await cleanupSandbox(sandbox, { success: true });
    }
  });
}

test('invalid reference records cannot prevent native cleanup', windowsOnly, async () => {
  const sandbox = await createSandbox({ runId: `support-reference-record-${randomUUID()}` });
  const record = path.join(sandbox.artifacts, 'credential-references.json');
  const reference = freshReference();
  try {
    await writeCredentials(sandbox, [reference]);
    await writeFile(record, '["not-an-e2e-reference"]');
    await assert.rejects(cleanupSandbox(sandbox, { success: false }), error => {
      assert.equal(error.cleanupResult.credentials.verified, true);
      assert.match(error.message, /Invalid recorded E2E credential references/);
      return true;
    });
    assert.ok(!(await snapshotCredentials(sandbox)).includes(`Agent Switchboard E2E.${reference}`));
  } finally {
    await writeFile(record, '[]');
    await cleanupSandbox(sandbox, { success: true });
  }
});

test('unsafe library junction is rejected while native credentials are cleaned', windowsOnly, async () => {
  const sandbox = await createSandbox({ runId: `support-library-junction-${randomUUID()}` });
  const link = path.join(sandbox.state, 'extensions');
  const sentinel = path.join(sandbox.fixtures, 'sentinel.json');
  const reference = freshReference();
  try {
    await writeFile(sentinel, '{"untouched":true}');
    await symlink(sandbox.fixtures, link, 'junction');
    await writeCredentials(sandbox, [reference]);
    await assert.rejects(cleanupSandbox(sandbox, { success: false }), error => {
      assert.ok(error.errors.some(failure => failure.stage === 'filesystem'));
      assert.ok(error.errors.some(failure => failure.stage === 'reference-scan'));
      assert.equal(error.cleanupResult.credentials.verified, true);
      return true;
    });
    assert.equal(await readFile(sentinel, 'utf8'), '{"untouched":true}');
    assert.ok(!(await snapshotCredentials(sandbox)).includes(`Agent Switchboard E2E.${reference}`));
  } finally {
    await rm(link);
    await cleanupSandbox(sandbox, { success: true });
  }
});

test('scan and native scope errors are both preserved, followed by an idempotent retry', windowsOnly, async () => {
  const sandbox = await createSandbox({ runId: `support-double-error-${randomUUID()}` });
  const baselineFile = path.join(sandbox.artifacts, 'credential-baseline.json');
  const baseline = await readFile(baselineFile, 'utf8');
  const reference = freshReference();
  const definition = await writeDefinition(sandbox, '{');
  let repaired = false;
  try {
    await writeCredentials(sandbox, [reference]);
    await writeFile(baselineFile, '["outside-e2e-namespace"]');
    await assert.rejects(cleanupSandbox(sandbox, { success: true }), error => {
      assert.deepEqual(error.errors.map(failure => failure.stage), ['reference-scan', 'credentials']);
      assert.equal(error.cleanupResult.credentials.verified, false);
      assert.ok(error.cleanupResult.credentials.errors.some(failure => failure.stage === 'baseline'));
      return true;
    });
    const firstEvidence = await readFile(path.join(sandbox.artifacts, 'credential-cleanup.json'), 'utf8');
    assert.ok((await snapshotCredentials(sandbox)).includes(`Agent Switchboard E2E.${reference}`));
    await writeFile(baselineFile, baseline);
    await rm(definition);
    repaired = true;
    const recovered = await cleanupSandbox(sandbox, { success: false });
    assert.equal(recovered.credentials.verified, true);
    assert.ok(recovered.credentials.deletedTargets.includes(`Agent Switchboard E2E.${reference}`));
    const attempts = await cleanupAttempts(sandbox);
    assert.equal(attempts.length, 2);
    assert.equal(attempts[0].errors.length, 2);
    assert.equal(attempts[1].errors.length, 0);
    const complete = await cleanupSandbox(sandbox, { success: true });
    assert.equal(complete.removed, true);
    assert.equal(await readFile(path.join(complete.artifacts, 'credential-cleanup.json'), 'utf8'), firstEvidence);
    assert.deepEqual(await cleanupSandbox(sandbox, { success: true }), complete);
  } finally {
    if (!repaired) {
      await writeFile(baselineFile, baseline);
      await rm(definition);
    }
    await cleanupSandbox(sandbox, { success: true });
  }
});

test('native cleanup preserves credentials present before the sandbox baseline', windowsOnly, async () => {
  const seed = await createSandbox({ runId: `support-baseline-seed-${randomUUID()}` });
  const preserved = freshReference();
  const added = freshReference();
  let sandbox;
  try {
    await writeCredentials(seed, [preserved]);
    sandbox = await createSandbox({ runId: `support-baseline-${randomUUID()}` });
    await writeCredentials(sandbox, [added]);
    await writeFile(path.join(sandbox.artifacts, 'credential-references.json'), JSON.stringify([preserved, added]));
    const result = await cleanupSandbox(sandbox, { success: false });
    assert.ok(!result.credentials.deletedTargets.includes(`Agent Switchboard E2E.${preserved}`));
    assert.ok(result.credentials.deletedTargets.includes(`Agent Switchboard E2E.${added}`));
    assert.ok((await snapshotCredentials(sandbox)).includes(`Agent Switchboard E2E.${preserved}`));
  } finally {
    if (sandbox) await cleanupSandbox(sandbox, { success: true });
    await cleanupSandbox(seed, { success: true });
  }
});

test('the native run lock excludes a second runner process', windowsOnly, async () => {
  const moduleUrl = new URL('./run-lock.mjs', import.meta.url).href;
  const source = `import { acquireRunLock, releaseRunLock } from ${JSON.stringify(moduleUrl)};
    try { const lock = await acquireRunLock(); await releaseRunLock(lock); }
    catch (error) { process.stderr.write(error.message); process.exitCode = 73; }`;
  await assert.rejects(execFileAsync(process.execPath, ['--input-type=module', '-e', source], {
    windowsHide: true, timeout: 15_000,
  }), error => error.code === 73 && /Another desktop E2E runner/.test(error.stderr));
});

test('release is idempotent and sandbox creation requires an active run lock', windowsOnly, async () => {
  const released = await releaseRunLock(runLock);
  assert.deepEqual(await releaseRunLock(runLock), released);
  await assert.rejects(createSandbox(), /Acquire the desktop E2E run lock/);
  runLock = await acquireRunLock();
  assert.ok(runLock.name.startsWith('Global\\AgentSwitchboard.DesktopE2E.'));
});
