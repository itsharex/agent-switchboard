import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { createHash, randomUUID } from 'node:crypto';
import { createWriteStream } from 'node:fs';
import { mkdir, readFile, readdir, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { createSandbox, assertSandbox, cleanupSandbox, acquireRunLock, releaseRunLock } from './support/sandbox.mjs';
import { seedFixtures } from './support/fixtures.mjs';
import { startNetworkGuard, startUpstream } from './support/upstream-server.mjs';
import { redact, redactArtifacts, snapshotFiles } from './support/assertions.mjs';
import { verifyBuildFingerprint } from './support/build-fingerprint.mjs';
import { assertNoDesktop, stopDesktop } from './support/desktop-process.mjs';

const repository = fileURLToPath(new URL('../', import.meta.url));
const executable = path.join(repository, 'target', 'e2e-desktop', 'release', 'Agent Switchboard E2E.exe');
const buildInputs = path.join(repository, 'target', 'e2e-desktop', 'build-inputs.json');
const argv = process.argv.slice(2);
const option = (name) => { const index = argv.indexOf(name); return index < 0 ? undefined : argv[index + 1]; };
const runId = `${new Date().toISOString().replace(/[^0-9]/g, '').slice(0, 14)}-${randomUUID().slice(0, 8)}`;

async function selectedSpecs() {
  const all = (await readdir(path.join(repository, 'e2e', 'specs'))).filter((name) => name.endsWith('.e2e.mjs'));
  const selection = option('--spec');
  const candidates = selection ? all.filter((name) => selection.split(',').some((filter) => name.includes(filter)))
    : all.filter((name) => argv.includes('--release') || !/clients|installed|network-release/.test(name));
  assert(candidates.length, 'No desktop specs matched');
  return candidates.sort((a, b) => Number(b.startsWith('tabs.')) - Number(a.startsWith('tabs.')) || a.localeCompare(b));
}

async function preflight() {
  assert.equal(process.platform, 'win32', 'Real desktop black-box tests require Windows');
  const manifest = JSON.parse((await readFile(path.join(repository, 'target', 'e2e-desktop', 'build-manifest.json'), 'utf8')).replace(/^\uFEFF/, ''));
  assert.equal(manifest.identifier, 'dev.agent-switchboard.desktop.e2e');
  assert.equal(manifest.feature, 'desktop-e2e');
  assert.equal(manifest.mode, 'release');
  assert.equal(createHash('sha256').update(await readFile(executable)).digest('hex'), manifest.sha256);
  const fingerprint = await verifyBuildFingerprint(buildInputs);
  assert.equal(manifest.sourceSha256, fingerprint.sha256, 'The successful build manifest does not belong to these source inputs');
  await assertNoDesktop(executable);
  return { ...manifest, sourceSha256: fingerprint.sha256 };
}

function childEnvironment(sandbox, spec, upstream) {
  const env = { ...sandbox.env };
  for (const key of Object.keys(env)) if (/^(http|https|all)_proxy$/i.test(key)) delete env[key];
  return { ...env, ASB_E2E_SANDBOX: JSON.stringify(sandbox), ASB_E2E_BINARY: executable,
    ASB_E2E_SPEC: path.join(repository, 'e2e', 'specs', spec), ASB_E2E_UPSTREAM_URL: upstream.url };
}

async function launchWdio(sandbox, spec, upstream) {
  const output = createWriteStream(path.join(sandbox.artifacts, 'runner.log'));
  const child = spawn(process.execPath, [path.join(repository, 'node_modules', '@wdio', 'cli', 'bin', 'wdio.js'), 'run', path.join(repository, 'e2e', 'wdio.conf.mjs')], {
    cwd: repository, env: childEnvironment(sandbox, spec, upstream), windowsHide: true, stdio: ['ignore', 'pipe', 'pipe'],
  });
  const relay = (chunk) => { output.write(chunk); process.stdout.write(redact(chunk.toString())); };
  child.stdout.on('data', relay);
  child.stderr.on('data', relay);
  const timer = setTimeout(() => child.kill(), 20 * 60 * 1000);
  try {
    return await new Promise((resolve, reject) => { child.once('error', reject); child.once('close', (code) => resolve(code ?? 1)); });
  } finally {
    clearTimeout(timer);
    await new Promise((resolve) => output.end(resolve));
  }
}

async function runSpec(spec, index, build) {
  const sandbox = await createSandbox({ runId: `${runId}-${index}-${spec.replace('.e2e.mjs', '')}` });
  let upstream, guard, status = 'failed', exitCode = 1, failure, cleanup;
  try {
    upstream = await startUpstream(sandbox);
    guard = await startNetworkGuard(sandbox);
    sandbox.networkGuardUrl = guard.url;
    for (const key of ['HTTP_PROXY', 'HTTPS_PROXY', 'ALL_PROXY', 'http_proxy', 'https_proxy', 'all_proxy']) sandbox.env[key] = guard.url;
    sandbox.env.no_proxy = sandbox.env.NO_PROXY;
    sandbox.env.WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = `--disable-background-networking --disable-component-update --no-first-run --proxy-server=${guard.url} --proxy-bypass-list=127.0.0.1;localhost;[::1]`;
    const fixture = await seedFixtures(sandbox, { upstreamUrl: upstream.url });
    if (spec.startsWith('sessions-usage-discovery')) {
      const key = Object.keys(sandbox.env).find((name) => name.toUpperCase() === 'PATH') ?? 'PATH';
      sandbox.env[key] = `${fixture.resumeBin}${path.delimiter}${sandbox.env[key] ?? ''}`;
    }
    await assertSandbox(sandbox);
    await writeFile(path.join(sandbox.artifacts, 'build.json'), JSON.stringify(build, null, 2));
    await writeFile(path.join(sandbox.artifacts, 'initial-files.json'), JSON.stringify(await snapshotFiles(sandbox.codex), null, 2));
    exitCode = await launchWdio(sandbox, spec, upstream);
    status = exitCode === 0 ? 'passed' : 'failed';
  } catch (error) { failure = error.stack; }
  finally {
    try { await stopDesktop(executable); } catch (error) { failure = `${failure ?? ''}\nDesktop cleanup: ${error.stack}`; status = 'failed'; }
    if (upstream) await upstream.close();
    if (guard) await guard.close();
    await writeFile(path.join(sandbox.artifacts, 'suite.json'), JSON.stringify({ spec, status, exitCode, failure: failure ? redact(failure) : null }, null, 2));
    await redactArtifacts(sandbox);
    try { cleanup = await cleanupSandbox(sandbox, { success: status === 'passed' }); }
    catch (error) { failure = `${failure ?? ''}\nSandbox cleanup: ${error.stack}`; status = 'failed'; }
  }
  return { spec, status, exitCode, error: failure ? redact(failure) : null, artifacts: cleanup?.artifacts ?? sandbox.artifacts };
}

async function main() {
  const specs = await selectedSpecs();
  const lock = await acquireRunLock();
  const results = [];
  const output = path.join(repository, 'target', 'e2e-reports', `${runId}-summary.json`);
  try {
    const build = await preflight();
    await mkdir(path.dirname(output), { recursive: true });
    for (const [index, spec] of specs.entries()) {
      console.log(`\nDesktop suite ${index + 1}/${specs.length}: ${spec}`);
      results.push(await runSpec(spec, index, build));
      await writeFile(output, JSON.stringify({ runId, build, results, completed: false }, null, 2));
    }
    let sourceUnchanged = true;
    try { await verifyBuildFingerprint(buildInputs); } catch { sourceUnchanged = false; }
    await writeFile(output, JSON.stringify({ runId, build, results, sourceUnchanged, completed: true }, null, 2));
    console.log(`Desktop evidence: ${output}`);
    process.exitCode = results.every((result) => result.status === 'passed') && sourceUnchanged ? 0 : 1;
  } finally { await releaseRunLock(lock); }
}

await main().catch((error) => { console.error(redact(error.stack)); process.exitCode = 1; });
