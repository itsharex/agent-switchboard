import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

const jsonl = (rows) => `${rows.map((row) => JSON.stringify(row)).join('\n')}\n`;

function timestamp(daysAgo) {
  const date = new Date();
  date.setDate(date.getDate() - daysAgo);
  date.setHours(12, 0, 0, 0);
  return date.toISOString();
}

function codexSession({ id, title, projectDir, at, model, input, cached, output }) {
  return jsonl([
    { type: 'session_meta', timestamp: at, payload: { id, cwd: projectDir }, customTitle: title },
    { type: 'turn_context', timestamp: at, payload: { model } },
    { type: 'response_item', timestamp: at, payload: { type: 'message', role: 'user', content: [{ type: 'input_text', text: `${title} user request` }] } },
    { type: 'response_item', timestamp: at, payload: { type: 'message', role: 'assistant', content: [{ type: 'output_text', text: `${title} deterministic reply` }] } },
    { type: 'event_msg', timestamp: at, payload: { type: 'token_count', info: { total_token_usage: { input_tokens: input, cached_input_tokens: cached, output_tokens: output, total_tokens: input + output } } } },
  ]);
}

function claudeSession({ id, title, projectDir, at, model, input, cached, output, created = 0 }) {
  return jsonl([
    { type: 'user', sessionId: id, cwd: projectDir, customTitle: title, timestamp: at, message: { role: 'user', content: `${title} user request` } },
    { type: 'assistant', sessionId: id, timestamp: at, message: { id: `${id}-message`, role: 'assistant', model, stop_reason: 'end_turn', content: [{ type: 'text', text: `${title} deterministic reply` }], usage: { input_tokens: input, cache_read_input_tokens: cached, cache_creation_input_tokens: created, output_tokens: output } } },
  ]);
}

export async function seedSessionFixtures(sandbox) {
  const projectDir = path.join(sandbox.fixtures, 'session-project');
  await mkdir(projectDir, { recursive: true });
  const specs = [
    { app: 'codex', id: 'e2e-codex-today', title: 'E2E Codex today', daysAgo: 0, model: 'e2e-gpt-today', input: 120, cached: 20, output: 30 },
    { app: 'claude', id: 'e2e-claude-today', title: 'E2E Claude today', daysAgo: 0, model: 'e2e-claude-today', input: 40, cached: 10, created: 5, output: 15 },
    { app: 'codex', id: 'e2e-codex-week', title: 'E2E Codex last week', daysAgo: 3, model: 'e2e-gpt-week', input: 70, cached: 10, output: 20 },
    { app: 'claude', id: 'e2e-claude-month', title: 'E2E Claude last month', daysAgo: 15, model: 'e2e-claude-month', input: 60, cached: 20, created: 10, output: 10 },
    { app: 'codex', id: 'e2e-codex-archive', title: 'E2E Codex archived', daysAgo: 45, model: 'e2e-gpt-archive', input: 80, cached: 15, output: 20 },
  ];
  const sessions = [];
  for (const spec of specs) {
    const root = spec.app === 'claude'
      ? path.join(sandbox.home, '.claude', 'projects', 'e2e-project')
      : path.join(sandbox.home, '.codex', spec.daysAgo === 45 ? 'archived_sessions' : 'sessions');
    await mkdir(root, { recursive: true });
    const file = path.join(root, `${spec.id}.jsonl`);
    const content = (spec.app === 'codex' ? codexSession : claudeSession)({ ...spec, projectDir, at: timestamp(spec.daysAgo) });
    await writeFile(file, content);
    sessions.push({ ...spec, file, content, projectDir });
  }
  return { sessions, totals: { today: 220, last7Days: 310, last30Days: 410, all: 510 } };
}

export async function seedResumeRecorder(sandbox) {
  const resumeBin = path.join(sandbox.fixtures, 'resume-bin');
  const resumeLog = path.join(sandbox.artifacts, 'session-resume.jsonl');
  const recorder = path.join(resumeBin, 'record-resume.cjs');
  await mkdir(resumeBin, { recursive: true });
  await writeFile(recorder, [
    "const fs = require('node:fs');",
    `const output = ${JSON.stringify(resumeLog)};`,
    'fs.appendFileSync(output, JSON.stringify({ argv: process.argv.slice(2), cwd: process.cwd(), pid: process.pid }) + "\\n");',
  ].join('\n'));
  for (const app of ['codex', 'claude']) {
    const script = `@echo off\r\n"${process.execPath}" "%~dp0record-resume.cjs" ${app} %*\r\nexit\r\n`;
    await writeFile(path.join(resumeBin, `${app}.cmd`), script);
  }
  return { resumeBin, resumeLog };
}

export async function appendUsageFixture(sandbox) {
  const file = path.join(sandbox.home, '.claude', 'projects', 'e2e-project', 'e2e-refresh.jsonl');
  const content = claudeSession({ id: 'e2e-refresh', title: 'E2E refresh record', projectDir: sandbox.fixtures, at: timestamp(0), model: 'e2e-refresh-model', input: 12, cached: 3, output: 5 });
  await writeFile(file, content);
  return { file, content, total: 20 };
}
