import assert from "node:assert/strict";
import { writeFile } from "node:fs/promises";
import { clickButton, fill, navigate, waitForText } from "../support/ui.mjs";
import {
  clientFile, clientSnapshot, jsonFile, openProviderDraft, providerFiles,
  providerRow, radio, readOptional, selectClient, switchProvider,
} from "../support/provider-ui.mjs";

const cases = {
  codex: { label: "在界面中隐藏推理摘要", key: "hide_agent_reasoning", protocol: "responses" },
  claude: { label: "默认开启扩展思考", key: "alwaysThinkingEnabled", protocol: "anthropicMessages" },
};

async function createWithParameters(app, suffix, choice) {
  assert(process.env.ASB_E2E_UPSTREAM_URL);
  const name = `E2E ${app} parameters ${suffix}`;
  await openProviderDraft({
    app, name, protocol: cases[app].protocol,
    baseUrl: `${process.env.ASB_E2E_UPSTREAM_URL}${app === "codex" ? "/v1" : ""}`,
    apiKey: "e2e-independent-parameters", model: `e2e-parameters-${suffix.replaceAll(" ", "-")}`,
  });
  assert.equal(await $(`[role="radiogroup"][aria-label="${cases[app].label}"]`).isExisting(), false);
  await clickButton("配置运行参数");
  await radio(cases[app].label, choice);
  if (app === "codex" && suffix === "enabled") {
    await $('//label[input[@aria-label="启用 1M 上下文窗口"]]').click();
  }
  await clickButton("返回供应商编辑");
  await expect($('form input[required]')).toHaveValue(name);
  await clickButton("保存供应商");
  await providerRow(name);
  const provider = (await providerFiles(app)).find((file) => file.name === name);
  assert(provider, "A complete current provider file must exist after saving");
  return provider;
}

async function assertClientParameter(app, expected) {
  if (app === "claude") {
    assert.equal((await jsonFile(clientFile(app)))[cases[app].key], expected);
    return;
  }
  const text = await readOptional(clientFile(app));
  if (expected === undefined) assert.doesNotMatch(text, /^hide_agent_reasoning\s*=/m);
  else assert.match(text, new RegExp(`^hide_agent_reasoning\\s*=\\s*${expected}`, "m"));
}

async function parameterIsolation(app) {
  const before = await clientSnapshot(app);
  const first = await createWithParameters(app, "enabled", "开启");
  const second = await createWithParameters(app, "disabled", "关闭");
  const automatic = await createWithParameters(app, "automatic", "自动");
  assert.deepEqual(await clientSnapshot(app), before);
  assert.deepEqual(first.parameters.settings[cases[app].key], { mode: "explicit", value: true });
  assert.deepEqual(second.parameters.settings[cases[app].key], { mode: "explicit", value: false });
  assert.deepEqual(automatic.parameters.settings[cases[app].key], { mode: "automatic" });
  if (app === "codex") assert.equal(first.modelOptions.contextWindow, 1_000_000);
  for (const [provider, expected] of [[first, true], [second, false], [automatic, undefined]]) {
    await switchProvider(provider.name);
    await assertClientParameter(app, expected);
    if (app === "codex") {
      const text = await readOptional(clientFile(app));
      if (provider.id === first.id) assert.match(text, /model_context_window\s*=\s*1000000/);
      else assert.doesNotMatch(text, /^model_context_window\s*=/m);
    }
  }
  assert.deepEqual((await jsonFile(first.filePath)).parameters, first.parameters);
  assert.deepEqual((await jsonFile(second.filePath)).parameters, second.parameters);
}

async function abandonParameters() {
  const provider = await createWithParameters("codex", "abandoned edit", "开启");
  const before = await readOptional(provider.filePath);
  const clientBefore = await clientSnapshot("codex");
  await navigate("供应商");
  await selectClient("codex", "供应商客户端");
  await providerRow(provider.name);
  await clickButton(`编辑 ${provider.name}`);
  await fill("名称", "Unsaved parent draft");
  await clickButton("配置运行参数");
  await radio(cases.codex.label, "关闭");
  await clickButton("返回供应商编辑");
  await expect($('form input[required]')).toHaveValue("Unsaved parent draft");
  await clickButton("配置运行参数");
  await expect($('[aria-label="在界面中隐藏推理摘要"] label.is-active')).toHaveText("关闭");
  await clickButton("返回供应商编辑");
  await clickButton("取消");
  assert.equal(await readOptional(provider.filePath), before);
  assert.deepEqual(await clientSnapshot("codex"), clientBefore);
}

async function activeParameterTransaction() {
  const provider = await createWithParameters("codex", "active transaction", "开启");
  await switchProvider(provider.name);
  const original = await readOptional(provider.filePath);
  const clientBefore = await clientSnapshot("codex");
  await providerRow(provider.name);
  await clickButton(`编辑 ${provider.name}`);
  await clickButton("配置运行参数");
  await radio(cases.codex.label, "关闭");
  await clickButton("返回供应商编辑");
  await clickButton("保存供应商");
  let sheet = await $('[role="dialog"][aria-label="确认保存并应用"]');
  await clickButton("取消", sheet);
  assert.equal(await readOptional(provider.filePath), original);
  assert.deepEqual(await clientSnapshot("codex"), clientBefore);
  await clickButton("保存供应商");
  sheet = await $('[role="dialog"][aria-label="确认保存并应用"]');
  await clickButton("确认保存并应用", sheet);
  await providerRow(provider.name);
  await assertClientParameter("codex", false);
  assert.deepEqual((await jsonFile(provider.filePath)).parameters.settings[cases.codex.key], { mode: "explicit", value: false });
  await clickButton(`编辑 ${provider.name}`);
  await clickButton("配置运行参数");
  await radio(cases.codex.label, "开启");
  await clickButton("返回供应商编辑");
  await clickButton("保存供应商");
  sheet = await $('[role="dialog"][aria-label="确认保存并应用"]');
  await sheet.waitForDisplayed();
  const current = await jsonFile(provider.filePath);
  const external = JSON.stringify({ ...current, notes: "outside-process parameter revision" }, null, 2);
  const applied = await clientSnapshot("codex");
  await writeFile(provider.filePath, external);
  await clickButton("确认保存并应用", sheet);
  await waitForText("供应商文件已被外部修改");
  assert.equal(await readOptional(provider.filePath), external);
  assert.deepEqual(await clientSnapshot("codex"), applied);
}

describe("Provider-owned runtime parameters through the desktop subview", () => {
  for (const app of Object.keys(cases)) {
    it(`${app} parameters persist per provider and automatic clears the preceding provider's value`, async () => {
      await parameterIsolation(app);
    });
  }
  it("returning retains parent and parameter drafts; cancelling the provider discards both", abandonParameters);
  it("active parameter edits support cancel, confirmed apply and external-revision failure without partial writes", activeParameterTransaction);
});
