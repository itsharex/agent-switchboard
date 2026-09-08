import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { seedCcSwitchFixture } from './fixture-ccswitch.mjs';
import { seedResumeRecorder, seedSessionFixtures } from './fixture-sessions.mjs';

async function write(file, content) {
  await mkdir(path.dirname(file), { recursive: true });
  await writeFile(file, content);
  return { file, content };
}

async function seedHostFiles(sandbox) {
  const codex = await write(path.join(sandbox.codex, 'config.toml'), [
    '# E2E native host configuration',
    'model = "gpt-e2e-host"',
    'approval_policy = "on-request"',
    'sandbox_mode = "workspace-write"',
    '',
    '[mcp_servers.e2e-native-mcp]',
    'command = "cmd.exe"',
    'args = ["/d", "/c", "echo e2e-native-mcp"]',
    '',
    '[mcp_servers.e2e-native-mcp.env]',
    'ASB_E2E_PUBLIC = "host-preserved"',
    '',
  ].join('\n'));
  const claude = await write(path.join(sandbox.claude, 'settings.json'), `${JSON.stringify({ env: { ASB_E2E_HOST: 'preserve-me' }, permissions: { allow: ['Read'] }, alwaysThinkingEnabled: true }, null, 2)}\n`);
  const claudeMcp = await write(path.join(sandbox.claude, '.claude.json'), `${JSON.stringify({ mcpServers: {}, hasCompletedOnboarding: true }, null, 2)}\n`);
  const codexPrompt = await write(path.join(sandbox.codex, 'AGENTS.md'), '# E2E Codex instructions\nPreserve this host-owned instruction.\n');
  const claudePrompt = await write(path.join(sandbox.claude, 'CLAUDE.md'), '# E2E Claude instructions\nPreserve this host-owned instruction.\n');
  return { codex, claude, claudeMcp, codexPrompt, claudePrompt };
}

async function seedSkills(sandbox) {
  const name = 'e2e-native-skill';
  const nativeRoot = path.join(sandbox.home, '.agents', 'skills', name);
  const manifest = await write(path.join(nativeRoot, 'SKILL.md'), `---\nname: ${name}\ndescription: An isolated native E2E skill.\n---\n\n# E2E native skill\n\nRead references/guide.md before answering.\n`);
  const guide = await write(path.join(nativeRoot, 'references', 'guide.md'), 'E2E skill guide, exact bytes preserved.\n');
  const invalid = await write(path.join(sandbox.home, '.agents', 'skills', 'e2e-invalid-skill', 'SKILL.md'), '# Deliberately missing frontmatter\nThis fixture produces one visible manual warning.\n');
  return { name, nativeRoot, manifest, guide, invalid, claudeTarget: path.join(sandbox.claude, 'skills', name) };
}

function discoveryFixture(host, upstreamUrl) {
  const claude = JSON.parse(host.claude.content);
  Object.assign(claude.env, { ANTHROPIC_BASE_URL: upstreamUrl, ANTHROPIC_AUTH_TOKEN: 'e2e-discovery-claude-secret', ANTHROPIC_MODEL: 'e2e-discovery-model' });
  return { claudeContent: `${JSON.stringify(claude, null, 2)}\n`, claudeName: '当前 Claude 配置', model: 'e2e-discovery-model', apiKey: claude.env.ANTHROPIC_AUTH_TOKEN, baseUrl: upstreamUrl };
}

/** Seed only client-owned inputs and external discovery sources. Application
 * profiles, settings, library records, caches, and transaction state begin empty. */
export async function seedFixtures(sandbox, { upstreamUrl = 'http://127.0.0.1:1' } = {}) {
  if (new URL(upstreamUrl).hostname !== '127.0.0.1') throw new Error('E2E fixture upstream must be IPv4 loopback');
  const host = await seedHostFiles(sandbox);
  const skills = await seedSkills(sandbox);
  const { sessions, totals } = await seedSessionFixtures(sandbox);
  const ccSwitch = await seedCcSwitchFixture(sandbox, upstreamUrl);
  const { resumeBin, resumeLog } = await seedResumeRecorder(sandbox);
  const result = { host, skills, sessions, totals, ccSwitch, resumeBin, resumeLog, discovery: discoveryFixture(host, upstreamUrl) };
  await write(path.join(sandbox.fixtures, 'manifest.json'), `${JSON.stringify(result, null, 2)}\n`);
  return result;
}
