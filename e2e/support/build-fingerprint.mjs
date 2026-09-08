import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdir, readFile, readdir, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const repository = fileURLToPath(new URL('../../', import.meta.url));
const roots = ['src', 'src-tauri', 'crates', 'public'];
const ignored = new Set(['target', 'node_modules', '.git']);
const sha256 = (bytes) => createHash('sha256').update(bytes).digest('hex');

export async function buildFingerprint() {
  const files = {};
  async function visit(relative) {
    for (const entry of await readdir(path.join(repository, relative), { withFileTypes: true }).catch((error) => { if (error.code === 'ENOENT') return []; throw error; })) {
      if (ignored.has(entry.name) || (entry.isDirectory() && entry.name.startsWith('target-'))) continue;
      const file = `${relative}/${entry.name}`;
      assert(!entry.isSymbolicLink(), `Build input contains a link: ${file}`);
      if (entry.isDirectory()) await visit(file);
      else files[file] = sha256(await readFile(path.join(repository, file)));
    }
  }
  for (const directory of roots) await visit(directory);
  for (const file of ['Cargo.toml', 'Cargo.lock', 'package.json', 'package-lock.json', 'index.html', 'tray.html', 'vite.config.ts', 'tsconfig.json', 'rust-toolchain.toml']) {
    files[file] = sha256(await readFile(path.join(repository, file)));
  }
  const sorted = Object.fromEntries(Object.entries(files).sort(([a], [b]) => a.localeCompare(b)));
  return { sha256: sha256(JSON.stringify(sorted)), files: sorted, capturedAt: new Date().toISOString() };
}

export async function verifyBuildFingerprint(file) {
  const expected = JSON.parse(await readFile(file, 'utf8'));
  const current = await buildFingerprint();
  const changed = [...new Set([...Object.keys(expected.files), ...Object.keys(current.files)])]
    .filter((name) => expected.files[name] !== current.files[name]);
  assert.equal(changed.length, 0, `Build inputs changed; rebuild before desktop testing: ${changed.join(', ')}`);
  return expected;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const file = path.join(repository, 'target', 'e2e-desktop', 'build-inputs.json');
  if (process.argv[2] === 'capture') {
    await mkdir(path.dirname(file), { recursive: true });
    await writeFile(file, `${JSON.stringify(await buildFingerprint(), null, 2)}\n`);
  } else if (process.argv[2] === 'verify') {
    await verifyBuildFingerprint(file);
  } else throw new Error('Expected capture or verify');
}
