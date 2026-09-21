import assert from "node:assert/strict";
import { test } from "node:test";
import type { CodexProviderRecord } from "../src/api/client.ts";
import { subagentModelKey, subagentModelOptions } from "../src/components/codex-provider-editor/subagent-model-options.ts";

function record(id: string, authBinding: object | null = null): CodexProviderRecord {
  return { profile: { id, name: id, connection: { authBinding }, catalog: [{ id: "shared-model" }] } } as CodexProviderRecord;
}

test("同名模型按供应商和模型的完整标识选择", () => {
  const choices = subagentModelOptions([record("provider-a"), record("provider-b")]);
  const selected = { profileId: "provider-b", model: "shared-model" };
  assert.notEqual(choices[0].value, choices[1].value);
  assert.deepEqual(choices.find(({ value }) => value === subagentModelKey(selected))?.route, selected);
});

test("跨供应商目录排除自身与账号绑定档案", () => {
  const choices = subagentModelOptions([record("self"), record("bound", {}), record("target")], "self");
  assert.deepEqual(choices.map(({ route }) => route.profileId), ["target"]);
});
