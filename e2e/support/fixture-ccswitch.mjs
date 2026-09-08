import { mkdir, readFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { DatabaseSync } from 'node:sqlite';
import path from 'node:path';

export async function seedCcSwitchFixture(sandbox, upstreamUrl) {
  const root = path.join(sandbox.home, '.cc-switch');
  await mkdir(root, { recursive: true });
  const file = path.join(root, 'cc-switch.db');
  const credential = 'e2e-ccswitch-opaque-credential';
  const db = new DatabaseSync(file);
  try {
    db.exec('CREATE TABLE providers (id TEXT, app_type TEXT, name TEXT, settings_config TEXT, website_url TEXT, notes TEXT, meta TEXT, PRIMARY KEY (id, app_type))');
    const insert = db.prepare('INSERT INTO providers VALUES (?, ?, ?, ?, ?, ?, ?)');
    insert.run('e2e-cc-claude', 'claude', 'E2E CC Switch Claude', JSON.stringify({ env: { ANTHROPIC_BASE_URL: upstreamUrl, ANTHROPIC_AUTH_TOKEN: credential, ANTHROPIC_MODEL: 'e2e-cc-claude-model' } }), upstreamUrl, 'Isolated E2E source', null);
    insert.run('e2e-cc-unsupported', 'gemini', 'E2E unsupported client', '{}', null, null, null);
  } finally {
    db.close();
  }
  return { file, credential, name: 'E2E CC Switch Claude', model: 'e2e-cc-claude-model', sha256: createHash('sha256').update(await readFile(file)).digest('hex') };
}
