import assert from "node:assert/strict";
import path from "node:path";
import { clickButton, fill, navigate, readSandbox } from "./ui.mjs";
import { requestRecords, snapshotFiles } from "./assertions.mjs";
import {
  clientSnapshot, createProvider, providerRow, providerSnapshot, selectClient,
} from "./provider-ui.mjs";

export const FIXED_PROMPT = "请只回复：连接成功。";
const targets = [
  { app: "codex", protocol: "responses", suffix: "/responses" },
  { app: "codex", protocol: "chatCompletions", suffix: "/chat/completions" },
  { app: "claude", protocol: "anthropicMessages", suffix: "/v1/messages" },
];

export async function createRequestProfiles() {
  const profiles = {};
  for (const target of targets) {
    const baseUrl = process.env.ASB_E2E_UPSTREAM_URL + (target.protocol === "anthropicMessages" ? "" : "/tenant/openai");
    const profile = await createProvider({
      ...target, name: `E2E direct request ${target.protocol}`, baseUrl,
      apiKey: `e2e-key-request-${target.protocol}`, model: `e2e-saved-${target.protocol}`,
    });
    profiles[target.protocol] = { ...profile, app: target.app, endpoint: baseUrl + target.suffix };
  }
  return profiles;
}

export async function unchangedRequestState() {
  const [codex, claude, codexProfiles, claudeProfiles, backups] = await Promise.all([
    clientSnapshot("codex"), clientSnapshot("claude"), providerSnapshot("codex"), providerSnapshot("claude"),
    snapshotFiles(path.join(readSandbox().state, "backups")),
  ]);
  return { codex, claude, codexProfiles, claudeProfiles, backups };
}

export async function openRequestPanel(profile) {
  await navigate("供应商");
  await selectClient(profile.app, "供应商客户端");
  await providerRow(profile.name);
  await clickButton(`${profile.name} 真实请求`);
  const panel = await $(`section[id="provider-request-${profile.id}"]`);
  await panel.waitForDisplayed();
  await expect(panel.$('[aria-label="请求结果"]')).toHaveAttribute("data-phase", "ready");
  assert.equal(await panel.$(".asb-request-message p").getText(), FIXED_PROMPT);
  assert((await panel.getText()).includes(profile.endpoint));
  assert((await panel.getText()).includes("本次验证不包含客户端切换或本机协议转换"));
  return panel;
}

export async function closeRequestPanels() {
  for (const panel of await $$("section.asb-request-panel")) {
    if (await panel.isDisplayed()) await clickButton("收起真实请求", panel);
  }
}

export async function sendPanelRequest(panel, model) {
  await fill("测试模型", model, panel);
  await clickButton("发送请求", panel);
}

export async function waitForNewRequest(start, model) {
  let found;
  await browser.waitUntil(async () => {
    found = (await requestRecords(readSandbox())).slice(start).find((record) => record.body?.model === model);
    return Boolean(found);
  }, { timeout: 15000, interval: 100, timeoutMsg: `No external fixture request received for ${model}` });
  return found;
}

export async function assertSingleDirectRequest(profile, start, model) {
  const requests = (await requestRecords(readSandbox())).slice(start);
  assert.equal(requests.length, 1, "One click must issue exactly one upstream request");
  const record = requests[0];
  assert.equal(record.method, "POST");
  assert.equal(record.url, new URL(profile.endpoint).pathname);
  assert.equal(record.body.model, model);
  assert.equal(record.body.stream, false);
  if (profile.upstreamProtocol === "responses") {
    assert.equal(record.body.input[0].content[0].text, FIXED_PROMPT);
  } else assert.equal(record.body.messages[0].content, FIXED_PROMPT);
  if (profile.upstreamProtocol === "anthropicMessages") {
    assert.equal(record.headers["x-api-key"], profile.apiKey);
    assert.equal(record.headers["anthropic-version"], "2023-06-01");
  } else assert.equal(record.headers.authorization, `Bearer ${profile.apiKey}`);
  assert(!JSON.stringify(record.headers).includes("asb_local_"), "The request panel must use the provider credential directly");
  return record;
}
