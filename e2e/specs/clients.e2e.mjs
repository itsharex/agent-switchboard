import assert from 'node:assert/strict';
import { readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { parse, stringify } from '@iarna/toml';
import { readClientProjection, requestRecords } from '../support/assertions.mjs';
import { createProvider, switchProvider } from '../support/provider-ui.mjs';
import { upstreamBase } from '../support/gateway-client.mjs';
import { ANSWER, REASONING } from '../support/protocol-fixtures.mjs';
import { CLI_PROMPT, runRealClient, verifyClientHelp } from '../support/real-clients.mjs';
import { openDiagnosticsSection, readSandbox } from '../support/ui.mjs';

const sandbox = readSandbox();
let clients;

async function assertCliProjection(app, protocol, direct, key) {
  const projection = await readClientProjection(sandbox, app);
  assert.equal(projection.model, 'e2e-model');
  if (direct) {
    assert.equal(projection.url, upstreamBase(protocol));
    assert.equal(projection.token, key);
    assert.equal(projection.config.model_providers.agent_switchboard.supports_websockets, false);
  } else {
    const gateway = JSON.parse(await readFile(path.join(sandbox.state, 'gateway.json'), 'utf8'));
    assert.equal(new URL(projection.url).port, String(gateway.port));
    assert.ok(projection.token.startsWith('asb_local_'));
    assert.ok(!projection.text.includes(key));
    assert.ok(!projection.text.includes(upstreamBase(protocol)));
  }
  return projection;
}

async function assertGatewayTelemetry(name, direct) {
  await openDiagnosticsSection('本机网关');
  const recent = await $('section[aria-label="最近请求"]');
  if (direct) {
    assert.ok(!(await recent.getText()).includes(name), 'A direct CLI request must not appear in gateway telemetry');
    return;
  }
  await browser.waitUntil(async () => (await recent.getText()).includes(name), { timeout: 15000 });
  assert.ok((await recent.getText()).includes('200'), 'The actual CLI exchange must have successful gateway telemetry');
}

async function exerciseClient({ app, protocol, direct = false, slug }) {
  const name = `E2E CLI ${slug}`;
  const key = `e2e-key-cli-${slug}`;
  await createProvider({ app, name, protocol, baseUrl: upstreamBase(protocol), apiKey: key, model: 'e2e-model',
    ...(protocol === 'responses' ? { responsesOptions: { requestMode: 'standard', supportsWebsockets: false } } : {}) });
  await switchProvider(name);
  const projection = await assertCliProjection(app, protocol, direct, key);
  const start = (await requestRecords(sandbox)).length;
  const blockedPath = path.join(sandbox.artifacts, 'blocked-network.jsonl');
  const blockedBefore = await readFile(blockedPath, 'utf8');
  const output = await runRealClient(sandbox, clients[app], slug);
  assert.equal(output.answer, ANSWER);
  for (const secret of [key, projection.token]) {
    assert.ok(!output.stdout.includes(secret), 'CLI stdout disclosed an authentication value');
    assert.ok(!output.stderr.includes(secret), 'CLI stderr disclosed an authentication value');
  }
  if (app === 'codex' && !direct) assert.ok(!output.stdout.includes(REASONING), 'Gateway reasoning must stay private');
  const requests = (await requestRecords(sandbox)).slice(start);
  const modelRequest = requests.find((record) => record.method === 'POST' && JSON.stringify(record.body).includes(CLI_PROMPT));
  assert.ok(modelRequest, 'The actual CLI did not reach the isolated upstream');
  const suffix = protocol === 'responses' ? '/responses' : '/chat/completions';
  assert.equal(new URL(modelRequest.url, upstreamBase(protocol)).pathname, new URL(upstreamBase(protocol)).pathname + suffix);
  assert.equal(modelRequest.headers.authorization, `Bearer ${key}`);
  assert.equal(modelRequest.body.model, 'e2e-model');
  assert.ok(!JSON.stringify(modelRequest.body).includes('asb_local_'));
  if (direct) assert.ok(requests.every((record) => record.method !== 'UPGRADE'), 'Responses WebSocket was disabled in the actual UI profile');
  assert.equal((await readClientProjection(sandbox, app)).text, projection.text, 'The CLI must retain the desktop-projected configuration');
  assert.equal(await readFile(blockedPath, 'utf8'), blockedBefore, 'The CLI attempted traffic outside the loopback fixtures');
  await assertGatewayTelemetry(name, direct);
}

describe('Pinned real Windows CLIs with desktop-created providers', () => {
  before(async () => {
    const configPath = path.join(sandbox.codex, 'config.toml');
    const host = parse(await readFile(configPath, 'utf8'));
    assert.equal(host.mcp_servers['e2e-native-mcp'].env.ASB_E2E_PUBLIC, 'host-preserved');
    delete host.mcp_servers['e2e-native-mcp'];
    await writeFile(configPath, stringify(host));
    assert.equal(parse(await readFile(configPath, 'utf8')).approval_policy, 'on-request');
    clients = await verifyClientHelp(sandbox);
  });

  it('Codex completes through the Chat Completions gateway', async () => {
    await exerciseClient({ app: 'codex', protocol: 'chatCompletions', slug: 'codex-chat' });
  });

  it('Claude completes through the Responses gateway', async () => {
    await exerciseClient({ app: 'claude', protocol: 'responses', slug: 'claude-responses' });
  });

  it('Claude completes through the Chat Completions gateway', async () => {
    await exerciseClient({ app: 'claude', protocol: 'chatCompletions', slug: 'claude-chat' });
  });

  it('Codex standard Responses uses direct HTTP with no WebSocket handshake', async () => {
    await exerciseClient({ app: 'codex', protocol: 'responses', direct: true, slug: 'codex-responses-http' });
  });
});
