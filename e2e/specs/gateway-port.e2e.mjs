import assert from "node:assert/strict";
import net from "node:net";
import { writeFile } from "node:fs/promises";
import path from "node:path";
import { clickButton, openDiagnosticsSection, readSandbox } from "../support/ui.mjs";
import {
  clientFile, clientSnapshot, createProvider, ensureSandboxPath, jsonFile,
  providerSnapshot, readOptional, switchProvider, waitForFile,
} from "../support/provider-ui.mjs";
import { readClientProjection, requestRecords, snapshotFiles } from "../support/assertions.mjs";
import { nativeRequest, requestThroughProjection } from "../support/gateway-client.mjs";
import { ANSWER } from "../support/protocol-fixtures.mjs";

async function reservePort(port = 0) {
  const server = net.createServer((socket) => socket.destroy());
  await new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen({ host: "127.0.0.1", port, exclusive: true }, resolve);
  });
  return {
    port: server.address().port,
    close: () => new Promise((resolve, reject) => server.close((error) => error ? reject(error) : resolve())),
  };
}

async function unusedPort() {
  const reservation = await reservePort();
  await reservation.close();
  return reservation.port;
}

async function canBind(port) {
  try {
    const listener = await reservePort(port);
    await listener.close();
    return true;
  } catch (error) {
    if (error.code === "EADDRINUSE") return false;
    throw error;
  }
}

async function portAcceptsConnections(port) {
  return new Promise((resolve, reject) => {
    const socket = net.createConnection({ host: "127.0.0.1", port });
    socket.once("connect", () => { socket.destroy(); resolve(true); });
    socket.once("error", (error) => {
      if (error.code === "ECONNREFUSED") resolve(false);
      else reject(error);
    });
    socket.setTimeout(1500, () => { socket.destroy(); reject(new Error(`Loopback port ${port} did not settle`)); });
  });
}

async function filesystemState() {
  const sandbox = readSandbox();
  const [codex, claude, gateway, codexProfiles, claudeProfiles, backups, journal] = await Promise.all([
    clientSnapshot("codex"), clientSnapshot("claude"), readOptional(path.join(sandbox.state, "gateway.json")),
    providerSnapshot("codex"), providerSnapshot("claude"), snapshotFiles(path.join(sandbox.state, "backups")),
    readOptional(path.join(sandbox.state, "gateway-port-journal.json")),
  ]);
  return { codex, claude, gateway, codexProfiles, claudeProfiles, backups, journal };
}

async function openPortInput(port) {
  await openDiagnosticsSection("本机网关");
  await clickButton("修改", await $('[aria-label="监听信息"]'));
  const sheet = await $('[role="dialog"][aria-label="修改监听端口"]');
  await sheet.waitForDisplayed();
  await sheet.$('input[type="number"]').setValue(String(port));
  await clickButton("下一步：预览变更", sheet);
  return sheet;
}

async function previewPort(port) {
  const sheet = await openPortInput(port);
  await sheet.$("button=确认修改并应用").waitForDisplayed();
  await expect(sheet).toHaveText(/E2E port codex/);
  await expect(sheet).toHaveText(/E2E port claude/);
  await expect(sheet).toHaveText(/本机能力令牌不轮换/);
  return sheet;
}

async function setupGatewayRoutes() {
  for (const app of ["codex", "claude"]) {
    const provider = await createProvider({
      app, name: `E2E port ${app}`, protocol: "chatCompletions",
      baseUrl: `${process.env.ASB_E2E_UPSTREAM_URL}/tenant/openai`,
      apiKey: `e2e-key-port-${app}`, model: "e2e-model",
    });
    await switchProvider(provider.name);
    const projection = await readClientProjection(readSandbox(), app);
    assert(projection.token.startsWith("asb_local_"));
    assert.notEqual(new URL(projection.url).port, new URL(process.env.ASB_E2E_UPSTREAM_URL).port);
  }
  await openDiagnosticsSection("本机网关");
  await expect($('[aria-label="网关运行状态"]')).toHaveText("运行中");
}

async function cancelledPreviewReleasesPort() {
  const before = await filesystemState();
  const port = await unusedPort();
  const sheet = await previewPort(port);
  assert.equal(await canBind(port), false, "A prepared preview must own the advertised port reservation");
  assert.deepEqual(await filesystemState(), before);
  await clickButton("取消", sheet);
  await sheet.waitForDisplayed({ reverse: true });
  await browser.waitUntil(() => canBind(port), { timeout: 3000, interval: 100,
    timeoutMsg: "Cancelling the visible preview did not release its reserved loopback port" });
  assert.deepEqual(await filesystemState(), before);
}

async function occupiedPortLeavesFilesUntouched() {
  const before = await filesystemState();
  const occupied = await reservePort();
  try {
    const sheet = await openPortInput(occupied.port);
    const error = await sheet.$('[role="alert"]');
    await error.waitForDisplayed();
    await expect(error).toHaveText(/端口已被占用/);
    await expect(sheet.$("button=确认修改并应用")).not.toExist();
    assert.deepEqual(await filesystemState(), before);
    await clickButton("取消", sheet);
  } finally {
    await occupied.close();
  }
}

async function assertPortOnlyProjection(before, after, app, port) {
  assert.equal(new URL(after.url).port, String(port));
  assert.equal(after.token, before.token, `${app} local capability token must remain stable`);
  const changed = structuredClone(after.config);
  if (app === "codex") {
    changed.model_providers.agent_switchboard.base_url = before.url;
    assert.deepEqual(after.auth, before.auth);
  } else changed.env.ANTHROPIC_BASE_URL = before.url;
  assert.deepEqual(changed, before.config, `${app} port transaction changed unrelated configuration`);
}

async function confirmPortUpdatesBothClients() {
  const sandbox = readSandbox();
  const before = await filesystemState();
  const originals = { codex: await readClientProjection(sandbox, "codex"), claude: await readClientProjection(sandbox, "claude") };
  const oldPort = Number(new URL(originals.codex.url).port);
  const port = await unusedPort();
  await clickButton("确认修改并应用", await previewPort(port));
  const sheet = await $('[role="dialog"][aria-label="修改监听端口"]');
  await expect(sheet).toHaveText(new RegExp(`监听端口已改为 ${port}`));
  await clickButton("关闭", sheet);
  const after = await filesystemState();
  assert.equal(JSON.parse(after.gateway).port, port);
  assert.deepEqual(after.codexProfiles, before.codexProfiles);
  assert.deepEqual(after.claudeProfiles, before.claudeProfiles);
  const backupRoot = path.join(sandbox.state, "backups");
  const added = Object.keys(after.backups).filter((file) => !(file in before.backups) && file.endsWith(".meta.json"));
  const records = await Promise.all(added.map((file) => jsonFile(path.join(backupRoot, file))));
  for (const app of ["codex", "claude"]) {
    await assertPortOnlyProjection(originals[app], await readClientProjection(sandbox, app), app, port);
    const backup = records.find((record) => record.targetPath === clientFile(app) && record.reason === "gateway-port-change");
    assert(backup, `The port change must back up ${app}'s actual previous document`);
    assert.equal(await readOptional(backup.backupPath), before[app].config);
    const marker = `E2E new port ${app}`;
    const result = await requestThroughProjection(sandbox, app, nativeRequest(app, marker));
    assert.equal(result.status, 200, result.text);
    assert(result.text.includes(ANSWER));
    const upstream = (await requestRecords(sandbox)).find((record) => JSON.stringify(record.body).includes(marker));
    assert.equal(upstream?.url, "/tenant/openai/chat/completions");
  }
  await browser.waitUntil(async () => !(await portAcceptsConnections(oldPort)), { timeout: 5000, interval: 100,
    timeoutMsg: "The previous gateway listener still accepts new connections" });
  assert.equal(await readOptional(path.join(sandbox.state, "gateway-port-journal.json")), null);
}

async function externalHostChangeRejectsPreparedPort() {
  const before = await filesystemState();
  const port = await unusedPort();
  const sheet = await previewPort(port);
  const external = `${before.codex.config}\n# E2E external host change after port preview\n`;
  await writeFile(ensureSandboxPath(clientFile("codex")), external);
  await clickButton("确认修改并应用", sheet);
  const error = await sheet.$('[role="alert"]');
  await error.waitForDisplayed();
  await expect(error).toHaveText(/预览后变化/);
  const after = await filesystemState();
  assert.deepEqual(after, { ...before, codex: { ...before.codex, config: external } });
  await clickButton("取消", sheet);
  await browser.waitUntil(() => canBind(port), { timeout: 3000, interval: 100,
    timeoutMsg: "A rejected one-shot preview must release its candidate port" });
  await waitForFile(clientFile("codex"), (text) => text === external);
}

describe("Gateway port changes through the real desktop UI and operating-system ports", () => {
  before(setupGatewayRoutes);
  it("cancelled port preview leaves every file unchanged and releases its reserved OS port", cancelledPreviewReleasesPort);
  it("an occupied OS port produces a visible error and no configuration/state/backup write", occupiedPortLeavesFilesUntouched);
  it("confirmation updates both client URLs with stable tokens, exact backups, usable new routes and a closed old port", confirmPortUpdatesBothClients);
  it("host changes after preview reject the port transaction and preserve all current documents", externalHostChangeRejectsPreparedPort);
});
