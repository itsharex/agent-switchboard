import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFile, readdir, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { parse } from '@iarna/toml';

const execFileAsync = promisify(execFile);

export async function readClientProjection(sandbox, app) {
  if (app === 'claude') {
    const text = await readFile(path.join(sandbox.claude, 'settings.json'), 'utf8');
    const config = JSON.parse(text);
    return { text, config, url: config.env.ANTHROPIC_BASE_URL, token: config.env.ANTHROPIC_AUTH_TOKEN, model: config.env.ANTHROPIC_MODEL };
  }
  const text = await readFile(path.join(sandbox.codex, 'config.toml'), 'utf8');
  const config = parse(text);
  const auth = JSON.parse(await readFile(path.join(sandbox.codex, 'auth.json'), 'utf8'));
  const provider = config.model_providers?.[config.model_provider];
  return { text, config, auth, url: provider?.base_url, token: auth.OPENAI_API_KEY, model: config.model };
}

export async function requestRecords(sandbox) {
  return (await readFile(path.join(sandbox.artifacts, 'upstream-requests.jsonl'), 'utf8'))
    .split('\n').filter(Boolean).map((line) => JSON.parse(line));
}

export async function loopbackFetch(url, options) {
  assert.equal(new URL(url).hostname, '127.0.0.1', 'Tests must never send model requests to external services');
  return fetch(url, { ...options, signal: AbortSignal.timeout(20000), redirect: 'error' });
}

export async function snapshotFiles(root) {
  const result = {};
  async function visit(directory) {
    for (const entry of await readdir(directory, { withFileTypes: true }).catch((error) => { if (error.code === 'ENOENT') return []; throw error; })) {
      const file = path.join(directory, entry.name);
      assert(!entry.isSymbolicLink(), `Unexpected fixture symlink: ${file}`);
      if (entry.isDirectory()) await visit(file);
      else result[path.relative(root, file)] = createHash('sha256').update(await readFile(file)).digest('hex');
    }
  }
  await visit(root);
  return result;
}

export async function captureProcessEvidence(sandbox, executable) {
  const script = [
    '$ErrorActionPreference = "Stop"',
    '$all = @(Get-CimInstance Win32_Process)',
    '$owned = @($all | Where-Object { $_.ExecutablePath -eq $env:ASB_E2E_BINARY -or ($_.Name -eq "msedgewebview2.exe" -and $_.CommandLine -like ("*" + $env:ASB_E2E_RUN_ROOT + "*")) })',
    '$pids = @($owned | ForEach-Object { $_.ProcessId })',
    '$connections = @(Get-NetTCPConnection -ErrorAction SilentlyContinue | Where-Object { $pids -contains $_.OwningProcess } | Select-Object OwningProcess,State,LocalAddress,LocalPort,RemoteAddress,RemotePort)',
    '@{ processes = @($owned | Select-Object Name,ProcessId,ParentProcessId,ExecutablePath,CommandLine); connections = $connections } | ConvertTo-Json -Depth 4 -Compress',
  ].join('\n');
  const { stdout } = await execFileAsync('powershell.exe', ['-NoLogo', '-NoProfile', '-NonInteractive', '-Command', script], {
    cwd: sandbox.root, env: { ...sandbox.env, ASB_E2E_BINARY: executable, ASB_E2E_RUN_ROOT: sandbox.root }, windowsHide: true, timeout: 30000,
  });
  const evidence = JSON.parse(stdout);
  assert(evidence.processes.some((entry) => entry.ExecutablePath?.toLowerCase() === executable.toLowerCase()), 'The actual E2E executable is not running');
  const webviews = evidence.processes.filter((entry) => entry.Name === 'msedgewebview2.exe');
  assert(webviews.length > 0, 'No real WebView2 process points at this sandbox');
  for (const connection of evidence.connections) {
    if (connection.State !== 5) continue;
    assert(['127.0.0.1', '::1', '0.0.0.0', '::'].includes(connection.RemoteAddress), `Observed non-loopback desktop connection: ${connection.RemoteAddress}`);
  }
  await writeFile(path.join(sandbox.artifacts, 'desktop-processes.json'), `${JSON.stringify(evidence, null, 2)}\n`);
  return evidence;
}

export function redact(text) {
  return text
    .replace(/asb_local_[a-f0-9]+/g, '<local-capability>')
    .replace(/e2e-(?:secret|key|token|discovery-claude-secret)[A-Za-z0-9_-]*/g, '<fixture-secret>')
    .replace(/("(?:authorization|x-api-key|apiKey|OPENAI_API_KEY|ANTHROPIC_AUTH_TOKEN)"\s*:\s*")[^"]*/gi, '$1<redacted>');
}

export async function redactArtifacts(sandbox) {
  async function visit(directory) {
    for (const entry of await readdir(directory, { withFileTypes: true })) {
      const file = path.join(directory, entry.name);
      if (entry.isDirectory()) await visit(file);
      else if (/\.(jsonl?|log|txt|html|xml)$/.test(entry.name)) {
        await writeFile(file, redact(await readFile(file, 'utf8')));
      }
    }
  }
  await visit(sandbox.artifacts);
}
