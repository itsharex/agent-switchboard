import assert from 'node:assert/strict';
import { spawn, execFile } from 'node:child_process';
import { createRequire } from 'node:module';
import { mkdir, readFile, stat, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { promisify } from 'node:util';
import { readClientProjection, redact } from './assertions.mjs';

const require = createRequire(import.meta.url);
const execFileAsync = promisify(execFile);
const repository = fileURLToPath(new URL('../../', import.meta.url));
export const CLI_PROMPT = 'Return exactly: E2E desktop answer';

export async function resolvePinnedClient(app) {
  assert.equal(process.platform, 'win32', 'This suite runs the actual Windows clients');
  assert.ok(['x64', 'arm64'].includes(process.arch));
  const packageName = { codex: '@openai/codex', claude: '@anthropic-ai/claude-code' }[app];
  assert.ok(packageName, 'Unsupported real client');
  const packagePath = require.resolve(`${packageName}/package.json`);
  const manifest = JSON.parse(await readFile(packagePath, 'utf8'));
  const dependencies = JSON.parse(await readFile(path.join(repository, 'package.json'), 'utf8')).devDependencies;
  assert.match(dependencies[packageName], /^\d+\.\d+\.\d+$/, 'Real CLI dependencies must use exact versions');
  assert.equal(manifest.version, dependencies[packageName], 'The installed CLI must match the pinned version');
  let executable;
  if (app === 'codex') {
    const platformRoot = path.dirname(require.resolve(`@openai/codex-win32-${process.arch}/package.json`));
    const triple = process.arch === 'x64' ? 'x86_64-pc-windows-msvc' : 'aarch64-pc-windows-msvc';
    executable = path.join(platformRoot, 'vendor', triple, 'bin', 'codex.exe');
  } else executable = path.join(path.dirname(packagePath), manifest.bin.claude);
  assert.equal(path.extname(executable), '.exe', 'Use the actual installed native executable');
  assert.ok((await stat(executable)).isFile());
  return { app, version: manifest.version, executable };
}

export function clientEnvironment(sandbox) {
  const env = { ...sandbox.env };
  for (const [key, expected] of Object.entries({ HOME: sandbox.home, USERPROFILE: sandbox.home,
    APPDATA: sandbox.appdata, LOCALAPPDATA: sandbox.localappdata, CODEX_HOME: sandbox.codex, CLAUDE_CONFIG_DIR: sandbox.claude })) {
    assert.equal(env[key], expected, `Real CLI environment must retain isolated ${key}`);
  }
  for (const key of Object.keys(env)) {
    assert.ok(!/^(?:OPENAI_API_KEY|ANTHROPIC_API_KEY|ANTHROPIC_AUTH_TOKEN|ANTHROPIC_BASE_URL|OPENAI_BASE_URL|CLAUDE_CODE_OAUTH_TOKEN|CLAUDECODE)$/i.test(key), `Unexpected inherited CLI identity: ${key}`);
  }
  assert.equal(new URL(sandbox.networkGuardUrl).hostname, '127.0.0.1', 'The real CLI requires the run-owned network guard');
  for (const key of ['HTTP_PROXY', 'HTTPS_PROXY', 'ALL_PROXY']) assert.equal(env[key], sandbox.networkGuardUrl);
  return { ...env, CI: '1', NO_PROXY: '127.0.0.1,localhost,::1',
    CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC: '1', DISABLE_TELEMETRY: '1',
    DISABLE_ERROR_REPORTING: '1', DISABLE_AUTOUPDATER: '1' };
}

async function stopOwnedTree(child, env) {
  if (child.exitCode !== null || child.signalCode !== null) return;
  assert.ok(Number.isInteger(child.pid) && child.pid > 0, 'Only an owned spawned PID may be stopped');
  const systemRoot = Object.entries(env).find(([key]) => key.toUpperCase() === 'SYSTEMROOT')?.[1];
  assert.ok(systemRoot);
  await execFileAsync(path.join(systemRoot, 'System32', 'taskkill.exe'), ['/PID', String(child.pid), '/T', '/F'], {
    env, windowsHide: true, timeout: 10000,
  });
}

function captureProcess(executable, args, { cwd, env, timeoutMs }) {
  return new Promise((resolve) => {
    const child = spawn(executable, args, { cwd, env, windowsHide: true, stdio: ['ignore', 'pipe', 'pipe'] });
    const startedAt = new Date().toISOString();
    const start = Date.now();
    let stdout = '';
    let stderr = '';
    let termination = null;
    let stopError = null;
    let spawnError = null;
    const result = (exitCode, signal) => ({ pid: child.pid, startedAt, durationMs: Date.now() - start,
      exitCode, signal, termination, stopError, spawnError, stdout, stderr });
    const stop = (reason) => {
      if (termination) return;
      termination = reason;
      void stopOwnedTree(child, env).catch((error) => {
        stopError = error.message;
        clearTimeout(timeout);
        resolve(result(child.exitCode, child.signalCode));
      });
    };
    const timeout = setTimeout(() => stop('timeout'), timeoutMs);
    const collect = (name, bytes) => {
      if (name === 'stdout') stdout += bytes.toString();
      else stderr += bytes.toString();
      if (Buffer.byteLength(stdout) + Buffer.byteLength(stderr) > 4 * 1024 * 1024) stop('output-limit');
    };
    child.stdout.on('data', (chunk) => collect('stdout', chunk));
    child.stderr.on('data', (chunk) => collect('stderr', chunk));
    child.on('error', (error) => { clearTimeout(timeout); spawnError = error.message; });
    child.on('close', (exitCode, signal) => {
      clearTimeout(timeout);
      resolve(result(exitCode, signal));
    });
  });
}

async function runCommand(sandbox, client, args, caseName, timeoutMs) {
  assert.match(caseName, /^[a-z0-9-]+$/);
  const cwd = path.join(sandbox.fixtures, 'real-clients', caseName);
  await mkdir(cwd, { recursive: true });
  const result = await captureProcess(client.executable, args, { cwd, env: clientEnvironment(sandbox), timeoutMs });
  const artifact = path.join(sandbox.artifacts, `cli-${caseName}.json`);
  await writeFile(artifact, `${redact(JSON.stringify({ ...client, args, cwd, ...result }, null, 2))}\n`);
  assert.equal(result.spawnError, null, `Real ${client.app} CLI failed to start; diagnostics: ${artifact}`);
  assert.equal(result.stopError, null, `Owned CLI PID ${result.pid} could not be terminated; diagnostics: ${artifact}`);
  assert.equal(result.termination, null, `Real ${client.app} CLI ${result.termination}; diagnostics: ${artifact}`);
  assert.equal(result.exitCode, 0, `Real ${client.app} CLI failed; diagnostics: ${artifact}\n${redact(result.stderr).slice(-2500)}`);
  return result;
}

export async function verifyClientHelp(sandbox) {
  const clients = {};
  for (const app of ['codex', 'claude']) {
    const client = await resolvePinnedClient(app);
    clients[app] = client;
    const version = await runCommand(sandbox, client, ['--version'], `${app}-version`, 15000);
    assert.ok(version.stdout.includes(client.version), 'Native CLI version differs from its installed manifest');
    const help = await runCommand(sandbox, client, app === 'codex' ? ['exec', '--help'] : ['--help'], `${app}-help`, 15000);
    const required = app === 'codex'
      ? ['--ephemeral', '--skip-git-repo-check', '--output-last-message', '--json', '--sandbox']
      : ['--print', '--output-format', '--setting-sources', '--strict-mcp-config', '--tools', '--disable-slash-commands'];
    for (const flag of required) assert.ok(help.stdout.includes(flag), `${app} help no longer supports ${flag}`);
  }
  return clients;
}

export async function runRealClient(sandbox, client, caseName) {
  const projection = await readClientProjection(sandbox, client.app);
  assert.equal(new URL(projection.url).hostname, '127.0.0.1', 'CLI model requests must use the UI-projected loopback endpoint');
  assert.ok(projection.token, 'CLI authentication must come from the UI-projected client file');
  const finalMessage = path.join(sandbox.fixtures, 'real-clients', caseName, 'last-message.txt');
  const args = client.app === 'codex'
    ? ['exec', '--ephemeral', '--skip-git-repo-check', '--ignore-rules', '--sandbox', 'read-only', '--json', '--color', 'never', '--output-last-message', finalMessage, CLI_PROMPT]
    : ['--print', '--output-format', 'json', '--no-session-persistence', '--setting-sources', 'user', '--strict-mcp-config', '--mcp-config', '{"mcpServers":{}}', '--disable-slash-commands', '--tools', '', '--permission-mode', 'dontAsk', '--permission-prompts', 'none', '--no-chrome', '--prompt-suggestions', 'false', CLI_PROMPT];
  const result = await runCommand(sandbox, client, args, caseName, 75000);
  const answer = client.app === 'codex' ? (await readFile(finalMessage, 'utf8')).trim() : JSON.parse(result.stdout).result;
  return { ...result, answer, projection };
}
