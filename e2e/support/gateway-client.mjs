import assert from 'node:assert/strict';
import path from 'node:path';
import { readFile } from 'node:fs/promises';
import { loopbackFetch, readClientProjection, requestRecords } from './assertions.mjs';
import { ANSWER, REASONING, TOOL_NAME } from './protocol-fixtures.mjs';

export const combinations = [
  { app: 'codex', protocol: 'responses', direct: true },
  { app: 'codex', protocol: 'chatCompletions', direct: false },
  { app: 'codex', protocol: 'anthropicMessages', direct: false },
  { app: 'claude', protocol: 'anthropicMessages', direct: true },
  { app: 'claude', protocol: 'responses', direct: false },
  { app: 'claude', protocol: 'chatCompletions', direct: false },
  { app: 'codex', protocol: 'responses', direct: false, minimal: true },
];

export function upstreamBase(protocol) {
  return process.env.ASB_E2E_UPSTREAM_URL + (protocol === 'anthropicMessages' ? '' : '/tenant/openai');
}

export function nativeRequest(app, marker, stream = false) {
  const schema = { type: 'object', properties: { value: { type: 'string' } }, required: ['value'] };
  if (app === 'codex') return {
    model: 'e2e-model', input: [{ role: 'user', content: [{ type: 'input_text', text: marker }] }],
    stream, max_output_tokens: 64,
    tools: [{ type: 'function', name: TOOL_NAME, description: 'A local fixture tool', parameters: schema, strict: false }], tool_choice: 'auto',
  };
  return { model: 'e2e-model', messages: [{ role: 'user', content: marker }], max_tokens: 64, stream,
    tools: [{ name: TOOL_NAME, description: 'A local fixture tool', input_schema: schema }], tool_choice: { type: 'auto' } };
}

export async function requestThroughProjection(sandbox, app, body) {
  const projection = await readClientProjection(sandbox, app);
  const url = `${projection.url.replace(/\/$/, '')}${app === 'codex' ? '/responses' : '/v1/messages'}`;
  const headers = { 'Content-Type': 'application/json' };
  if (app === 'codex') headers.Authorization = `Bearer ${projection.token}`;
  else { headers['x-api-key'] = projection.token; headers['anthropic-version'] = '2023-06-01'; }
  const response = await loopbackFetch(url, { method: 'POST', headers, body: JSON.stringify(body) });
  return { status: response.status, headers: Object.fromEntries(response.headers), text: await response.text(), projection };
}

export async function assertProjection(sandbox, item, key) {
  const projection = await readClientProjection(sandbox, item.app);
  assert.equal(projection.model, 'e2e-model');
  if (item.app === 'codex') {
    assert.equal(projection.config.model_provider, 'agent_switchboard');
    assert.equal(projection.config.model_providers.agent_switchboard.wire_api, 'responses');
    assert.equal(projection.config.mcp_servers['e2e-native-mcp'].env.ASB_E2E_PUBLIC, 'host-preserved');
    assert.equal(projection.config.openai_base_url, undefined);
  } else assert.equal(projection.config.env.ASB_E2E_HOST, 'preserve-me');
  if (item.direct) {
    assert.equal(projection.url, upstreamBase(item.protocol));
    assert.equal(projection.token, key);
  } else {
    const state = JSON.parse(await readFile(path.join(sandbox.state, 'gateway.json'), 'utf8'));
    assert.equal(new URL(projection.url).hostname, '127.0.0.1');
    assert.equal(new URL(projection.url).port, String(state.port));
    assert(projection.token.startsWith('asb_local_'));
    assert(!projection.text.includes(key));
    assert(!projection.text.includes(upstreamBase(item.protocol)));
    assert(!JSON.stringify(projection.auth ?? {}).includes(key));
  }
  return projection;
}

export async function assertUpstream(sandbox, item, key, marker) {
  const records = await requestRecords(sandbox);
  const record = [...records].reverse().find((row) => JSON.stringify(row.body).includes(marker));
  assert(record, 'No actual upstream request was received');
  const suffix = { responses: '/responses', chatCompletions: '/chat/completions', anthropicMessages: '/v1/messages' }[item.protocol];
  assert.equal(record.url, new URL(upstreamBase(item.protocol)).pathname.replace(/\/$/, '') + suffix);
  if (item.protocol === 'anthropicMessages') {
    assert.equal(record.headers['x-api-key'], key);
    assert.equal(record.headers['anthropic-version'], '2023-06-01');
    assert.equal(record.body.max_tokens, 64);
  } else assert.equal(record.headers.authorization, `Bearer ${key}`);
  assert.equal(record.body.model, 'e2e-model');
  assert(!JSON.stringify(record.body).includes('asb_local_'));
  return record;
}

export function assertResponse(result, item, { tools = false, stream = false } = {}) {
  assert.equal(result.status, 200, result.text);
  assert(result.text.includes(ANSWER), 'The response did not return model text');
  if (tools) assert(result.text.includes(TOOL_NAME), 'The response did not return the requested tool call');
  if (stream) {
    assert(result.headers['content-type'].includes('text/event-stream'));
    assert(result.text.includes(item.app === 'codex' ? 'response.completed' : 'message_stop'));
  } else {
    const json = JSON.parse(result.text);
    assert.equal(json.usage.input_tokens, 11);
    assert.equal(json.usage.output_tokens, 7);
  }
  if (item.app === 'codex' && !item.direct) {
    assert(!result.text.includes(REASONING), 'Gateway leaked private reasoning as client-visible text');
    assert(result.text.includes('encrypted_content'), 'Reasoning continuation was lost');
  }
}
