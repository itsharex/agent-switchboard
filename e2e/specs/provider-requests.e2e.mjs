import assert from "node:assert/strict";
import { clickButton, fill, readSandbox } from "../support/ui.mjs";
import { requestRecords } from "../support/assertions.mjs";
import { ANSWER } from "../support/protocol-fixtures.mjs";
import {
  assertSingleDirectRequest, closeRequestPanels, createRequestProfiles, openRequestPanel,
  sendPanelRequest, unchangedRequestState, waitForNewRequest,
} from "../support/provider-request-ui.mjs";

let profiles;
let baseline;

async function setupProfiles() {
  profiles = await createRequestProfiles();
  baseline = await unchangedRequestState();
}

async function preparationAndCloseDoNotSend() {
  const before = await requestRecords(readSandbox());
  for (const profile of Object.values(profiles)) {
    const panel = await openRequestPanel(profile);
    await fill("测试模型", "", panel);
    await expect(panel.$("button=发送请求")).toBeDisabled();
    assert.deepEqual(await requestRecords(readSandbox()), before);
    await closeRequestPanels();
    assert.deepEqual(await requestRecords(readSandbox()), before);
  }
}

async function succeedsDirectly(protocol) {
  const profile = profiles[protocol];
  const panel = await openRequestPanel(profile);
  const start = (await requestRecords(readSandbox())).length;
  const model = `e2e-requested-${protocol}`;
  await sendPanelRequest(panel, model);
  const result = await panel.$('[aria-label="请求结果"]');
  await expect(result).toHaveAttribute("data-phase", "success");
  await expect(result.$("blockquote")).toHaveText(ANSWER);
  await expect(result).toHaveText(/HTTP 200/);
  await expect(result).toHaveText(/模型 e2e-model/);
  assert(!(await result.getText()).includes(model), "The receipt must show the returned model, not echo the requested ID");
  await assertSingleDirectRequest(profile, start, model);
}

async function httpFailure(status, title, modelSuffix = "") {
  const profile = profiles.responses;
  const panel = await openRequestPanel(profile);
  const start = (await requestRecords(readSandbox())).length;
  const model = `e2e-http-${status}${modelSuffix}`;
  await sendPanelRequest(panel, model);
  const result = await panel.$('[aria-label="请求结果"]');
  await expect(result).toHaveAttribute("data-phase", "failure");
  await expect(result.$("h4")).toHaveText(title);
  await expect(result).toHaveText(new RegExp(`HTTP ${status}`));
  await expect(result).toHaveText(new RegExp(`req-e2e-${status}`));
  assert((await result.getText()).includes(profile.endpoint));
  await result.$("details summary").click();
  const diagnosticBody = await result.$("pre").getText();
  assert(diagnosticBody.includes(`E2E deterministic HTTP ${status}`));
  assert(diagnosticBody.includes("credential="), "Error details must retain useful upstream context");
  assert(!diagnosticBody.includes(profile.apiKey), "The upstream echo of the credential must be redacted");
  assert(!(await panel.getText()).includes(profile.apiKey));
  await assertSingleDirectRequest(profile, start, model);
}

async function unexpectedMalformedSseIsAnError() {
  const profile = profiles.responses;
  const panel = await openRequestPanel(profile);
  const start = (await requestRecords(readSandbox())).length;
  await sendPanelRequest(panel, "e2e-bad-stream");
  const result = await panel.$('[aria-label="请求结果"]');
  await expect(result).toHaveAttribute("data-phase", "failure");
  await expect(result.$("h4")).toHaveText("响应解析失败");
  await expect(result).toHaveText(/HTTP 200/);
  await expect(result).toHaveText(/req-e2e-stream/);
  await expect(result.$("blockquote")).not.toExist();
  await result.$("details summary").click();
  await expect(result.$("pre")).toHaveText(/broken json/);
  await assertSingleDirectRequest(profile, start, "e2e-bad-stream");
}

async function cancellationCannotOverwriteNextReply() {
  const profile = profiles.responses;
  const panel = await openRequestPanel(profile);
  const start = (await requestRecords(readSandbox())).length;
  await sendPanelRequest(panel, "e2e-delay");
  const delayed = await waitForNewRequest(start, "e2e-delay");
  const result = await panel.$('[aria-label="请求结果"]');
  await expect(result).toHaveAttribute("data-phase", "sending");
  await clickButton("取消请求", panel);
  await expect(result).toHaveAttribute("data-phase", "cancelled");
  await expect(result).toHaveText(/请求已取消/);
  await assertSingleDirectRequest(profile, start, "e2e-delay");
  const nextStart = (await requestRecords(readSandbox())).length;
  await sendPanelRequest(panel, "e2e-after-cancel");
  await expect(result).toHaveAttribute("data-phase", "success");
  const completed = await result.getText();
  await assertSingleDirectRequest(profile, nextStart, "e2e-after-cancel");
  // The fixture finishes its discarded response after ten seconds.
  await browser.waitUntil(() => Date.now() - Date.parse(delayed.at) >= 11000, { timeout: 13000, interval: 200 });
  await expect(result).toHaveAttribute("data-phase", "success");
  assert.equal(await result.getText(), completed, "A cancelled attempt must not replace the later receipt");
  assert.equal((await requestRecords(readSandbox())).length, start + 2);
}

describe("Provider request panel: direct upstream evidence without client switching", () => {
  before(setupProfiles);
  afterEach(async () => {
    await closeRequestPanels();
    assert.deepEqual(await unchangedRequestState(), baseline, "Direct request validation must not mutate client files, providers or backups");
  });
  it("opening, editing and closing a prepared request sends nothing and an empty model disables send", preparationAndCloseDoNotSend);
  for (const protocol of ["responses", "chatCompletions", "anthropicMessages"]) {
    it(`${protocol} sends one direct authenticated request with the fixed prompt and displays the actual reply/model`, async () => {
      await succeedsDirectly(protocol);
    });
  }
  for (const [status, title, suffix] of [
    [401, "认证失败", ""], [400, "请求参数错误", ""], [404, "API 路径错误", ""],
    [404, "模型不存在", "-model"], [429, "请求受限", ""], [500, "供应商上游错误", ""],
  ]) {
    it(`HTTP ${status}${suffix} preserves diagnostic status/Request ID and redacts the echoed provider key`, async () => {
      await httpFailure(status, title, suffix);
    });
  }
  it("unexpected malformed SSE is visibly rejected as an invalid response, with its HTTP status and Request ID", unexpectedMalformedSseIsAnError);
  it("cancelling a recorded in-flight request permits another send and its delayed completion cannot overwrite the newer reply", cancellationCannotOverwriteNextReply);
});
