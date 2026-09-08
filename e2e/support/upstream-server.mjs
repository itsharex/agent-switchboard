import http from 'node:http';
import { appendFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { responseFor, streamFor } from './protocol-fixtures.mjs';

const PROTOCOLS = { '/responses': 'responses', '/chat/completions': 'chatCompletions', '/v1/messages': 'anthropicMessages' };

export async function startUpstream(sandbox) {
  const requestFile = path.join(sandbox.artifacts, 'upstream-requests.jsonl');
  await writeFile(requestFile, '');
  const server = http.createServer((request, response) => {
    handleRequest(request, response, requestFile).catch((error) => {
      response.writeHead(500).end(JSON.stringify({ error: error.message }));
    });
  });
  server.on('upgrade', async (request, socket) => {
    await appendFile(requestFile, `${JSON.stringify({ at: new Date().toISOString(), method: 'UPGRADE', url: request.url, headers: request.headers })}\n`);
    socket.end('HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nx-request-id: req-e2e-websocket\r\n\r\n');
  });
  await new Promise((resolve, reject) => {
    server.once('error', reject);
    server.listen(0, '127.0.0.1', resolve);
  });
  return { url: `http://127.0.0.1:${server.address().port}`, requestFile, close: () => new Promise((resolve) => { server.closeAllConnections(); server.close(resolve); }) };
}

async function handleRequest(request, response, requestFile) {
  const chunks = [];
  for await (const chunk of request) {
    chunks.push(chunk);
    if (chunks.reduce((length, item) => length + item.length, 0) > 4 * 1024 * 1024) throw new Error('Fixture request exceeds limit');
  }
  const text = Buffer.concat(chunks).toString();
  const body = text ? JSON.parse(text) : null;
  await appendFile(requestFile, `${JSON.stringify({ at: new Date().toISOString(), method: request.method, url: request.url, headers: request.headers, body })}\n`);
  const pathname = new URL(request.url, 'http://127.0.0.1').pathname;
  if (pathname.endsWith('/models')) {
    response.writeHead(200, { 'Content-Type': 'application/json' }).end(JSON.stringify({ object: 'list', data: [{ id: 'e2e-model', object: 'model', owned_by: 'desktop-fixture' }] }));
    return;
  }
  const protocol = Object.entries(PROTOCOLS).find(([suffix]) => pathname.endsWith(suffix))?.[1];
  if (!protocol || request.method !== 'POST') { response.writeHead(404).end(); return; }
  const failure = text.includes('E2E_FAILURE') ? 429 : Number(body.model?.match(/^e2e-http-(\d+)/)?.[1]);
  if (failure) {
    response.writeHead(failure, { 'Content-Type': 'application/json', 'x-request-id': `req-e2e-${failure}` }).end(JSON.stringify({ type: 'error', error: { type: failure === 429 ? 'rate_limit_error' : 'invalid_request_error', code: failure === 404 && body.model?.includes('model') ? 'model_not_found' : 'e2e_failure', message: `E2E deterministic HTTP ${failure}; credential=${request.headers['x-api-key'] ?? request.headers.authorization}` } }));
    return;
  }
  if (body.model === 'e2e-delay') await new Promise((resolve) => setTimeout(resolve, 10000));
  if (body.model === 'e2e-bad-stream') {
    response.writeHead(200, { 'Content-Type': 'text/event-stream', 'x-request-id': 'req-e2e-stream' }).end('data: {broken json}\n\n');
    return;
  }
  if (!body.stream) {
    response.writeHead(200, { 'Content-Type': 'application/json' }).end(JSON.stringify(responseFor(protocol, body)));
    return;
  }
  response.writeHead(200, { 'Content-Type': 'text/event-stream', 'Cache-Control': 'no-cache' });
  for (const event of streamFor(protocol, body)) response.write(event);
  response.end();
}

export async function startNetworkGuard(sandbox) {
  const file = path.join(sandbox.artifacts, 'blocked-network.jsonl');
  await writeFile(file, '');
  const server = http.createServer(async (request, response) => {
    await appendFile(file, `${JSON.stringify({ method: request.method, url: request.url })}\n`);
    response.writeHead(403).end('Desktop E2E allows loopback fixture traffic only');
  });
  server.on('connect', async (request, socket) => {
    await appendFile(file, `${JSON.stringify({ method: 'CONNECT', url: request.url })}\n`);
    socket.end('HTTP/1.1 403 Forbidden\r\n\r\n');
  });
  await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
  return { url: `http://127.0.0.1:${server.address().port}`, file, close: () => new Promise((resolve) => { server.closeAllConnections(); server.close(resolve); }) };
}
