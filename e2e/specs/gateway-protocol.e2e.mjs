import assert from 'node:assert/strict';
import { writeFile } from 'node:fs/promises';
import path from 'node:path';
import { createProvider, switchProvider } from '../support/provider-ui.mjs';
import { openDiagnosticsSection, readSandbox } from '../support/ui.mjs';
import { combinations, upstreamBase, nativeRequest, requestThroughProjection, assertProjection, assertUpstream, assertResponse } from '../support/gateway-client.mjs';

describe('Real desktop client/upstream protocol matrix', () => {
  const sandbox = readSandbox();
  for (const item of combinations) {
    it(`${item.app} to ${item.protocol}${item.minimal ? ' minimal' : ''}: UI switch, native files, text, SSE, tools, reasoning and failure`, async () => {
      const name = `E2E ${item.app} ${item.protocol}${item.minimal ? ' minimal' : ''}`;
      const key = `e2e-key-${item.app}-${item.protocol}${item.minimal ? '-minimal' : ''}`;
      await createProvider({ app: item.app, name, protocol: item.protocol, baseUrl: upstreamBase(item.protocol), apiKey: key, model: 'e2e-model',
        ...(item.app === 'codex' && item.protocol === 'anthropicMessages' ? { maxOutputTokens: 512 } : {}),
        ...(item.minimal ? { responsesOptions: { requestMode: 'minimal', supportsWebsockets: false } } : {}) });
      await switchProvider(name);
      await assertProjection(sandbox, item, key);
      const evidence = [];
      for (const [kind, stream] of [['E2E_TEXT', false], ['E2E_STREAM', true], ['E2E_TOOLS', false], ['E2E_TOOLS_STREAM', true]]) {
        const marker = `${kind}_${item.app}_${item.protocol}${item.minimal ? '_minimal' : ''}`;
        const result = await requestThroughProjection(sandbox, item.app, nativeRequest(item.app, marker, stream));
        assertResponse(result, item, { tools: kind.includes('TOOLS'), stream });
        await assertUpstream(sandbox, item, key, marker);
        evidence.push({ kind, status: result.status, headers: result.headers, text: result.text });
      }
      const failure = await requestThroughProjection(sandbox, item.app, nativeRequest(item.app, 'E2E_FAILURE'));
      assert.equal(failure.status, 429);
      assert.equal(failure.headers['x-request-id'], 'req-e2e-429');
      if (!item.direct) assert(!failure.text.includes(key));
      await writeFile(path.join(sandbox.artifacts, `protocol-${item.app}-${item.protocol}${item.minimal ? '-minimal' : ''}.json`), JSON.stringify(evidence, null, 2));
      if (!item.direct) {
        await openDiagnosticsSection('本机网关');
        await browser.waitUntil(async () => (await $('section[aria-label="最近请求"]').getText()).includes(name), { timeout: 15000 });
        const text = await $('section[aria-label="最近请求"]').getText();
        assert(text.includes('429'));
        assert(text.includes('200'));
      }
    });
  }

  it('minimal Responses omits optional fields and rejects implicit context before any upstream request', async () => {
    const body = { ...nativeRequest('codex', 'E2E_MINIMAL_FIELDS'), reasoning: { effort: 'high' }, service_tier: 'priority', store: false,
      include: ['reasoning.encrypted_content'], metadata: { fixture: true }, client_metadata: { fixture: true }, prompt_cache_key: 'e2e-cache' };
    const result = await requestThroughProjection(sandbox, 'codex', body);
    assert.equal(result.status, 200, result.text);
    const record = await assertUpstream(sandbox, combinations.at(-1), 'e2e-key-codex-responses-minimal', 'E2E_MINIMAL_FIELDS');
    for (const name of ['reasoning', 'service_tier', 'store', 'include', 'metadata', 'client_metadata', 'prompt_cache_key']) assert.equal(record.body[name], undefined);
    const rejected = await requestThroughProjection(sandbox, 'codex', { ...body, previous_response_id: 'resp_implicit' });
    assert.equal(rejected.status, 400);
    assert(rejected.text.includes('previous_response_id'));
  });
});
