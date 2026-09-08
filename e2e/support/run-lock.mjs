import { spawn } from 'node:child_process';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const locks = new WeakMap();
let activeLock;

function startMutexProcess() {
  if (process.platform !== 'win32') throw new Error('Desktop E2E requires a Windows run lock');
  const systemRoot = Object.entries(process.env).find(([key]) => key.toUpperCase() === 'SYSTEMROOT')?.[1];
  if (!systemRoot) throw new Error('Windows SystemRoot is required for the desktop run lock');
  const executable = path.join(systemRoot, 'System32', 'WindowsPowerShell', 'v1.0', 'powershell.exe');
  const child = spawn(executable, [
    '-NoLogo', '-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass', '-File',
    fileURLToPath(new URL('./run-lock.ps1', import.meta.url)),
  ], { windowsHide: true, stdio: ['pipe', 'pipe', 'pipe'] });
  let diagnostics = '';
  child.stderr.on('data', chunk => { diagnostics += chunk.toString(); });
  const exited = new Promise(resolve => {
    child.once('error', error => resolve({ code: -1, diagnostics: error.message }));
    child.once('close', code => resolve({ code, diagnostics }));
  });
  return { child, exited };
}

async function mutexReady(handle) {
  let timer;
  let text = '';
  const firstLine = new Promise((resolve, reject) => {
    handle.child.stdout.on('data', chunk => {
      text += chunk.toString();
      const line = text.split(/\r?\n/)[0];
      if (!text.includes('\n')) return;
      try { resolve(JSON.parse(line)); }
      catch (error) { reject(error); }
    });
  });
  try {
    return await Promise.race([
      firstLine,
      handle.exited.then(result => { throw new Error(`Run lock process exited: ${result.code} ${result.diagnostics}`); }),
      new Promise((_, reject) => { timer = setTimeout(() => reject(new Error('Run lock timed out')), 10_000); }),
    ]);
  } finally { clearTimeout(timer); }
}

export async function acquireRunLock() {
  const handle = startMutexProcess();
  try {
    const ready = await mutexReady(handle);
    if (!ready.acquired) throw new Error(`Another desktop E2E runner holds ${ready.name}`);
    const lock = Object.freeze({ name: ready.name, ownerPid: ready.ownerPid, abandoned: ready.abandoned });
    locks.set(lock, { ...handle, release: null });
    activeLock = lock;
    return lock;
  } catch (error) {
    handle.child.stdin.end();
    handle.child.kill();
    await handle.exited;
    throw error;
  }
}

export function assertRunLock() {
  const handle = activeLock && locks.get(activeLock);
  if (!handle || handle.release || handle.child.exitCode !== null) {
    throw new Error('Acquire the desktop E2E run lock before creating or cleaning a sandbox');
  }
}

export async function releaseRunLock(lock) {
  const handle = locks.get(lock);
  if (!handle) throw new Error('Cannot release a desktop run lock owned by another caller');
  if (!handle.release) {
    handle.release = (async () => {
      handle.child.stdin.end('release\n');
      const result = await handle.exited;
      if (activeLock === lock) activeLock = undefined;
      if (result.code !== 0) throw new Error(`Run lock release failed: ${result.code} ${result.diagnostics}`);
      return { released: true, name: lock.name };
    })();
  }
  return handle.release;
}
