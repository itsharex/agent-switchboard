import { execFile } from 'node:child_process';
import { randomUUID } from 'node:crypto';
import { cp, lstat, mkdir, readFile, readdir, realpath, rm, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { promisify } from 'node:util';
import { assertRunLock } from './run-lock.mjs';

export { acquireRunLock, releaseRunLock } from './run-lock.mjs';

const execFileAsync = promisify(execFile);
const repository = path.resolve(fileURLToPath(new URL('../../', import.meta.url)));
const runDirectory = path.join(repository, 'target', 'e2e-runs');
const identifier = 'dev.agent-switchboard.desktop.e2e';
const credentialPrefix = 'Agent Switchboard E2E.';
const secretReference = /^secret-[a-f0-9]{32}$/;
const systemEnvironment = new Set([
  'PATH', 'PATHEXT', 'SYSTEMROOT', 'SYSTEMDRIVE', 'WINDIR', 'COMSPEC', 'OS',
  'NUMBER_OF_PROCESSORS', 'PROCESSOR_ARCHITECTURE', 'PROCESSOR_ARCHITEW6432',
  'PROCESSOR_IDENTIFIER', 'PROCESSOR_LEVEL', 'PROCESSOR_REVISION', 'COMPUTERNAME',
  'USERNAME', 'USERDOMAIN', 'USERDOMAIN_ROAMINGPROFILE', 'SESSIONNAME', 'LOGONSERVER',
  'PROGRAMFILES', 'PROGRAMFILES(X86)', 'PROGRAMW6432', 'COMMONPROGRAMFILES',
  'COMMONPROGRAMFILES(X86)', 'COMMONPROGRAMW6432', 'PROGRAMDATA', 'ALLUSERSPROFILE',
  'LANG', 'LC_ALL', 'LC_CTYPE', 'TZ', 'TERM', 'NO_COLOR', 'FORCE_COLOR',
]);

function isInside(root, candidate, allowRoot = false) {
  const relative = path.relative(root, candidate);
  return (allowRoot || relative !== '') && !relative.startsWith(`..${path.sep}`)
    && relative !== '..' && !path.isAbsolute(relative);
}

function directories(root) {
  const appdata = path.join(root, 'appdata');
  const localappdata = path.join(root, 'localappdata');
  return {
    root, home: path.join(root, 'home'), appdata, localappdata,
    codex: path.join(root, 'codex'), claude: path.join(root, 'claude'),
    fixtures: path.join(root, 'fixtures'), artifacts: path.join(root, 'artifacts'),
    state: path.join(appdata, identifier, 'state'),
    logs: path.join(localappdata, identifier, 'logs'),
    webview: path.join(localappdata, identifier, 'webview'),
    temp: path.join(root, 'temp'),
  };
}

function childEnvironment(sandbox) {
  const env = Object.fromEntries(Object.entries(process.env)
    .filter(([key]) => systemEnvironment.has(key.toUpperCase())));
  return {
    ...env,
    USERPROFILE: sandbox.home, HOME: sandbox.home,
    HOMEDRIVE: path.parse(sandbox.home).root.slice(0, 2), HOMEPATH: sandbox.home.slice(2),
    APPDATA: sandbox.appdata, LOCALAPPDATA: sandbox.localappdata,
    CODEX_HOME: sandbox.codex, CLAUDE_CONFIG_DIR: sandbox.claude,
    CARGO_TARGET_DIR: path.join(sandbox.root, 'cargo-target'),
    CARGO_HOME: path.join(sandbox.home, '.cargo'),
    TEMP: sandbox.temp, TMP: sandbox.temp, TMPDIR: sandbox.temp,
    WEBVIEW2_USER_DATA_FOLDER: sandbox.webview,
    XDG_CONFIG_HOME: path.join(sandbox.home, '.config'),
    XDG_DATA_HOME: path.join(sandbox.home, '.local', 'share'),
    XDG_CACHE_HOME: path.join(sandbox.root, 'cache'),
    NPM_CONFIG_CACHE: path.join(sandbox.root, 'cache', 'npm'),
    NPM_CONFIG_USERCONFIG: path.join(sandbox.home, '.npmrc'),
    NPM_CONFIG_GLOBALCONFIG: path.join(sandbox.home, '.npmrc-global'),
    GIT_CONFIG_NOSYSTEM: '1', GIT_CONFIG_GLOBAL: path.join(sandbox.home, '.gitconfig'),
    NO_PROXY: '127.0.0.1,localhost,::1',
    CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC: '1', DISABLE_TELEMETRY: '1',
    DISABLE_ERROR_REPORTING: '1', DISABLE_AUTOUPDATER: '1',
  };
}

async function assertDirectoryChain(root, candidate) {
  if (!isInside(root, candidate, true)) throw new Error(`Sandbox path escapes its root: ${candidate}`);
  const relative = path.relative(root, candidate);
  let current = root;
  for (const component of ['', ...relative.split(path.sep).filter(Boolean)]) {
    current = component ? path.join(current, component) : current;
    const info = await lstat(current).catch(error => {
      if (error.code === 'ENOENT') return null;
      throw error;
    });
    if (!info) break;
    if (info.isSymbolicLink()) throw new Error(`Sandbox path contains a symlink or junction: ${current}`);
    if (!info.isDirectory()) throw new Error(`Sandbox directory is not a directory: ${current}`);
  }
}

async function createDirectory(candidate) {
  await assertDirectoryChain(repository, candidate);
  await mkdir(candidate, { recursive: true });
  const resolved = await realpath(candidate);
  if (!isInside(await realpath(repository), resolved)) throw new Error(`Resolved sandbox path escaped: ${resolved}`);
}

async function credentialCommand(sandbox, mode, manifest, input) {
  if (process.platform !== 'win32') throw new Error('Desktop E2E requires Windows Credential Manager');
  const script = fileURLToPath(new URL('./windows-credentials.ps1', import.meta.url));
  const args = ['-NoLogo', '-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass', '-File', script, '-Mode', mode];
  if (manifest) {
    await writeFile(input, `${JSON.stringify(manifest, null, 2)}\n`, { flag: 'wx' });
    args.push('-InputPath', input);
  }
  const systemRoot = Object.entries(sandbox.env).find(([key]) => key.toUpperCase() === 'SYSTEMROOT')?.[1];
  if (!systemRoot) throw new Error('Windows SystemRoot is required for credential verification');
  const executable = path.join(systemRoot, 'System32', 'WindowsPowerShell', 'v1.0', 'powershell.exe');
  let result;
  try {
    result = await execFileAsync(executable, args, {
      cwd: sandbox.root, env: sandbox.env, windowsHide: true, timeout: 30_000, maxBuffer: 4 * 1024 * 1024,
    });
  } catch (cause) {
    let details;
    try { details = JSON.parse(cause.stdout.trim()); } catch { /* Native diagnostics may precede JSON. */ }
    const message = details?.errors?.map(error => error.message).join('; ') || cause.stderr?.trim() || cause.message;
    const error = new Error(`Windows credential ${mode.toLowerCase()} failed: ${message}`, { cause });
    error.details = details;
    throw error;
  }
  if (result.stderr.trim()) throw new Error(`Credential verification wrote diagnostics: ${result.stderr.trim()}`);
  return JSON.parse(result.stdout.trim());
}

export async function createSandbox({ runId = `${Date.now()}-${randomUUID().slice(0, 8)}` } = {}) {
  if (!/^[a-zA-Z0-9][a-zA-Z0-9_-]{0,95}$/.test(runId)) throw new Error('Invalid E2E run ID');
  assertRunLock();
  const sandbox = directories(path.join(runDirectory, runId));
  await createDirectory(runDirectory);
  await mkdir(sandbox.root);
  for (const directory of Object.values(sandbox).filter(directory => directory !== sandbox.root)) {
    await createDirectory(directory);
  }
  sandbox.env = childEnvironment(sandbox);
  await writeFile(path.join(sandbox.root, '.sandbox.json'), `${JSON.stringify({ version: 1, runId, root: sandbox.root })}\n`, { flag: 'wx' });
  const baseline = await credentialCommand(sandbox, 'Snapshot');
  if (!Array.isArray(baseline) || baseline.some(target => !target.startsWith(credentialPrefix))) {
    throw new Error('Windows credential snapshot escaped the E2E namespace');
  }
  await writeFile(path.join(sandbox.artifacts, 'credential-baseline.json'), `${JSON.stringify(baseline, null, 2)}\n`, { flag: 'wx' });
  await assertSandbox(sandbox);
  return sandbox;
}

function sandboxRoot(sandbox) {
  const root = path.resolve(sandbox.root);
  if (path.dirname(root) !== runDirectory) throw new Error('Sandbox must be a direct child of target/e2e-runs');
  for (const [name, expected] of Object.entries(directories(root))) {
    if (sandbox[name] !== expected) throw new Error(`Sandbox ${name} does not match the directory contract`);
  }
  return root;
}

async function assertOwnership(sandbox) {
  const root = sandboxRoot(sandbox);
  await assertDirectoryChain(repository, root);
  const marker = await readJsonIfPresent(path.join(root, '.sandbox.json'), null);
  if (marker?.version !== 1 || marker.root !== root || marker.runId !== path.basename(root)) {
    throw new Error('Sandbox ownership marker does not match its directory');
  }
  for (const expected of Object.values(directories(root))) await assertDirectoryChain(root, expected);
  return root;
}

export async function assertSandbox(sandbox) {
  const root = await assertOwnership(sandbox);
  assertEnvironment(sandbox);
  await assertTree(root);
  await recordSandboxSecrets(sandbox);
  const isolation = {
    identifier, paths: directories(root),
    evidence: ['absolute paths checked', 'symlinks and junctions rejected', 'child environment isolated', 'E2E credential namespace snapshot'],
    processFileAccessTrace: 'Not collected by sandbox preflight; environment checks alone do not prove zero external file reads.',
  };
  await writeFile(path.join(sandbox.artifacts, 'isolation.json'), `${JSON.stringify(isolation, null, 2)}\n`);
  return isolation;
}

function assertEnvironment(sandbox) {
  const expected = childEnvironment(sandbox);
  for (const key of ['USERPROFILE', 'HOME', 'APPDATA', 'LOCALAPPDATA', 'CODEX_HOME',
    'CLAUDE_CONFIG_DIR', 'CARGO_TARGET_DIR', 'TEMP', 'TMP', 'TMPDIR', 'WEBVIEW2_USER_DATA_FOLDER']) {
    if (sandbox.env[key] !== expected[key]) throw new Error(`Sandbox environment escaped through ${key}`);
  }
  for (const key of Object.keys(sandbox.env)) {
    if (/^(?:HTTP|HTTPS|ALL)_PROXY$/i.test(key)) {
      const value = sandbox.env[key];
      if (!sandbox.networkGuardUrl || value !== sandbox.networkGuardUrl
        || new URL(value).hostname !== '127.0.0.1' || new URL(value).protocol !== 'http:') {
        throw new Error(`Sandbox proxy does not point to its loopback network guard: ${key}`);
      }
    } else if (/^ASB_WEB_DEVELOPMENT$/i.test(key)
      || /(?:API[_-]?KEY|ACCESS[_-]?TOKEN|AUTH[_-]?TOKEN|SECRET[_-]?KEY)$/i.test(key)) {
      throw new Error(`Production or development environment variable reached the sandbox: ${key}`);
    }
  }
}

async function readJsonIfPresent(file, fallback) {
  try {
    if ((await lstat(file)).isSymbolicLink()) throw new Error(`Refusing to read a JSON symlink: ${file}`);
    return JSON.parse(await readFile(file, 'utf8'));
  } catch (error) {
    if (error.code === 'ENOENT') return fallback;
    if (error instanceof SyntaxError) throw new Error(`Malformed JSON: ${file}`, { cause: error });
    throw error;
  }
}

function collectReferences(value, references) {
  if (!value || typeof value !== 'object') return;
  if (value.kind === 'secretRef') throw new Error('Extension library uses obsolete kind:secretRef instead of mode:secretRef');
  if (value.mode === 'secretRef') {
    if (typeof value.reference !== 'string' || !secretReference.test(value.reference)) {
      throw new Error('Extension library contains an invalid credential reference');
    }
    references.add(value.reference);
  }
  for (const child of Object.values(value)) collectReferences(child, references);
}

async function collectLibraryReferences(directory, references) {
  const entries = await readdir(directory, { withFileTypes: true }).catch(error => {
    if (error.code === 'ENOENT') return [];
    throw error;
  });
  for (const entry of entries) {
    const file = path.join(directory, entry.name);
    if (entry.isSymbolicLink()) throw new Error(`Extension library contains a symlink: ${file}`);
    if (entry.isDirectory()) await collectLibraryReferences(file, references);
    else if (entry.name.endsWith('.json')) collectReferences(await readJsonIfPresent(file, null), references);
  }
}

export async function recordSandboxSecrets(sandbox) {
  await assertOwnership(sandbox);
  await assertDirectoryChain(sandbox.root, path.join(sandbox.state, 'extensions'));
  const recorded = path.join(sandbox.artifacts, 'credential-references.json');
  const values = await readJsonIfPresent(recorded, []);
  if (!Array.isArray(values) || values.some(value => typeof value !== 'string' || !secretReference.test(value))) {
    throw new Error('Invalid recorded E2E credential references');
  }
  const references = new Set(values);
  await collectLibraryReferences(path.join(sandbox.state, 'extensions'), references);
  await writeFile(recorded, `${JSON.stringify([...references].sort(), null, 2)}\n`);
  return [...references].sort();
}

async function assertTree(root) {
  for (const entry of await readdir(root, { withFileTypes: true })) {
    const candidate = path.join(root, entry.name);
    if (entry.isSymbolicLink()) throw new Error(`Sandbox cleanup rejected a symlink: ${candidate}`);
    if (entry.isDirectory()) await assertTree(candidate);
  }
}

async function writeFirst(file, value) {
  try { await writeFile(file, `${JSON.stringify(value, null, 2)}\n`, { flag: 'wx' }); }
  catch (error) { if (error.code !== 'EEXIST') throw error; }
}

function cleanupError(stage, cause) {
  const error = new Error(`${stage}: ${cause.message}`, { cause });
  error.stage = stage;
  error.details = cause.details;
  return error;
}

async function completedCleanup(sandbox) {
  const root = sandboxRoot(sandbox);
  await assertDirectoryChain(repository, root);
  const exists = await lstat(root).catch(error => {
    if (error.code === 'ENOENT') return null;
    throw error;
  });
  if (exists) return null;
  const artifacts = path.join(repository, 'target', 'e2e-reports', path.basename(root));
  await assertDirectoryChain(repository, artifacts);
  const result = await readJsonIfPresent(path.join(artifacts, 'cleanup-result.json'), null);
  if (!result?.removed || result.root !== root || result.artifacts !== artifacts || !result.credentials?.verified) {
    throw new Error('Sandbox disappeared without a verified cleanup receipt');
  }
  return result;
}

async function archiveSandbox(sandbox, credentials, attemptDirectory) {
  await cp(sandbox.logs, path.join(attemptDirectory, 'runtime-logs'), { recursive: true, errorOnExist: true, force: false });
  const reports = path.join(repository, 'target', 'e2e-reports');
  await createDirectory(reports);
  const artifacts = path.join(reports, path.basename(sandbox.root));
  await assertDirectoryChain(repository, artifacts);
  await mkdir(artifacts).catch(error => { if (error.code !== 'EEXIST') throw error; });
  const owner = path.join(artifacts, '.sandbox-archive.json');
  await writeFirst(owner, { root: sandbox.root });
  if ((await readJsonIfPresent(owner, null))?.root !== sandbox.root) throw new Error('Report directory belongs to another sandbox');
  await cp(sandbox.artifacts, artifacts, { recursive: true, errorOnExist: false, force: false });
  await assertDirectoryChain(repository, sandbox.root);
  await assertTree(sandbox.root);
  await rm(sandbox.root, { recursive: true, force: false });
  const result = { removed: true, root: sandbox.root, artifacts, credentials };
  await writeFirst(path.join(artifacts, 'cleanup-result.json'), result);
  return result;
}

export async function cleanupSandbox(sandbox, { success } = {}) {
  assertRunLock();
  const completed = await completedCleanup(sandbox);
  if (completed) return completed;
  await assertOwnership(sandbox);
  const attempt = `${Date.now()}-${randomUUID()}`;
  const attemptDirectory = path.join(sandbox.artifacts, 'cleanup-attempts', attempt);
  await createDirectory(attemptDirectory);
  const errors = [];
  let references = [];
  let credentials = null;
  try { assertEnvironment(sandbox); } catch (error) { errors.push(cleanupError('environment', error)); }
  try { await assertTree(sandbox.root); } catch (error) { errors.push(cleanupError('filesystem', error)); }
  try { references = await recordSandboxSecrets(sandbox); }
  catch (error) { errors.push(cleanupError('reference-scan', error)); }
  try {
    const baseline = await readJsonIfPresent(path.join(sandbox.artifacts, 'credential-baseline.json'), null);
    const nativeSandbox = { ...sandbox, env: childEnvironment(sandbox) };
    credentials = await credentialCommand(nativeSandbox, 'Cleanup', { baseline, references }, path.join(attemptDirectory, 'input.json'));
    if (!credentials.verified) throw new Error('Windows credential cleanup was not verified');
  } catch (error) {
    credentials = error.details || credentials;
    errors.push(cleanupError('credentials', error));
  }
  const evidence = {
    attempt, requestedSuccess: Boolean(success), referenceCount: references.length, credentials,
    errors: errors.map(error => ({ stage: error.stage, message: error.message, details: error.details })),
    outcome: errors.length ? 'retained-after-errors' : success ? 'ready-to-archive' : 'retained',
  };
  await writeFirst(path.join(attemptDirectory, 'result.json'), evidence);
  await writeFirst(path.join(sandbox.artifacts, 'credential-cleanup.json'), credentials || { verified: false, errors: evidence.errors });
  const result = { removed: false, root: sandbox.root, artifacts: sandbox.artifacts, credentials };
  if (errors.length) {
    const error = new AggregateError(errors, `Sandbox cleanup failed: ${errors.map(error => error.message).join('; ')}`);
    error.cleanupResult = result;
    throw error;
  }
  if (!success) return result;
  try { return await archiveSandbox(sandbox, credentials, attemptDirectory); }
  catch (error) {
    await writeFirst(path.join(attemptDirectory, 'archive-error.json'), { message: error.message });
    error.cleanupResult = result;
    throw error;
  }
}
