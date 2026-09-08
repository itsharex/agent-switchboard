import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const repository = fileURLToPath(new URL('../../', import.meta.url));
const clientPackages = ['@openai/codex', '@anthropic-ai/claude-code'];

// This module owns the names reserved for release-only test resources. A
// declared name is not proof that a resource exists or is safe to use.
const releaseResources = [
  {
    id: 'cloud-backup',
    description: 'Dedicated Supabase test project and account, with per-run rows and verified deletion',
    requiredEnvironment: [
      'ASB_E2E_SUPABASE_URL', 'ASB_E2E_SUPABASE_ANON_KEY',
      'ASB_E2E_SUPABASE_EMAIL', 'ASB_E2E_SUPABASE_PASSWORD',
    ],
    unimplementedScenarios: ['Cloud backup upload, ciphertext inspection, restore and remote-row cleanup'],
  },
  {
    id: 'updater',
    description: 'Test signing keys and a local HTTPS update source',
    requiredEnvironment: [
      'ASB_E2E_UPDATER_PRIVATE_KEY_PATH', 'ASB_E2E_UPDATER_PUBLIC_KEY_PATH',
      'ASB_E2E_UPDATER_HTTPS_CERT_PATH', 'ASB_E2E_UPDATER_HTTPS_KEY_PATH',
      'ASB_E2E_UPDATER_BASE_URL',
    ],
    unimplementedScenarios: ['No-update, available-update, verified download and native restart flows'],
  },
  {
    id: 'installed-application',
    description: 'Generated installer and a disposable Windows VM',
    requiredEnvironment: ['ASB_E2E_INSTALLER_PATH', 'ASB_E2E_DISPOSABLE_VM_ID'],
    unimplementedScenarios: ['Install, launch the installed executable, navigate all tabs, switch a provider and reset the VM'],
  },
];

function inspectPinnedClient(packageName, dependencies) {
  const pinnedVersion = dependencies[packageName];
  let installedVersion = null;
  try {
    const manifest = path.join(repository, 'node_modules', packageName, 'package.json');
    installedVersion = JSON.parse(readFileSync(manifest, 'utf8')).version;
  } catch { /* An absent or unreadable installed manifest remains a missing prerequisite. */ }
  const satisfied = /^\d+\.\d+\.\d+$/.test(pinnedVersion ?? '') && installedVersion === pinnedVersion;
  return {
    id: packageName, description: 'Repository-pinned real CLI package', satisfied,
    pinnedVersion: pinnedVersion ?? null, installedVersion,
    evidence: 'Package manifests only; desktop and gateway execution belongs to clients.e2e.mjs',
    missingEnvironment: [], unimplementedScenarios: [],
  };
}

export function inspectReleasePrerequisites(env = process.env) {
  const names = new Set(Object.getOwnPropertyNames(env).map(name => name.toUpperCase()));
  const dependencies = JSON.parse(readFileSync(path.join(repository, 'package.json'), 'utf8')).devDependencies ?? {};
  const checks = clientPackages.map(packageName => inspectPinnedClient(packageName, dependencies));
  for (const resource of releaseResources) {
    checks.push({
      ...resource,
      // These scenarios do not exist yet. Environment variables must never
      // turn an empty release suite into a successful release gate.
      satisfied: false,
      missingEnvironment: resource.requiredEnvironment.filter(name => !names.has(name)),
      declaredEnvironment: resource.requiredEnvironment.filter(name => names.has(name)),
    });
  }
  return {
    ready: checks.every(check => check.satisfied),
    checks,
    missing: checks.filter(check => !check.satisfied).map(check => ({
      id: check.id, description: check.description, missingEnvironment: check.missingEnvironment,
      unimplementedScenarios: check.unimplementedScenarios,
    })),
    evidenceScope: 'Environment names and local package manifests only; no secret values, remote services or registry were read.',
  };
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const report = inspectReleasePrerequisites();
  console.log(JSON.stringify(report, null, 2));
  process.exitCode = report.ready ? 0 : 1;
}
