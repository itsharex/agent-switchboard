import { expect, it } from "vitest";
import type {
  AuthenticationScheme,
  ClaudeModelSettings,
  ModelOptions,
  ProviderDraft,
  ProviderProfile,
  UpstreamProtocol,
} from "../../api/client";
import { claudeOptions, draftFrom, prepareDraft, type ProviderEditorDraft } from "./draft";

const protocols: UpstreamProtocol[] = ["anthropicMessages", "chatCompletions", "responses"];
const authenticationCases: Array<AuthenticationScheme | null | undefined> = [
  "bearer", "xApiKey", null, undefined,
];
const connections = protocols.flatMap((protocol) =>
  authenticationCases.map((authentication) => ({ protocol, authentication })),
);

it("normalizes omitted backend option fields without changing the form contract", () => {
  const source = profileFor("anthropicMessages", "bearer");
  delete (source as Partial<ProviderProfile>).responsesOptions;
  delete (source as Partial<ProviderProfile>).modelOptions;
  const draft = draftFrom(source, "claude");
  const saved = savedDraft(draft);
  expect(saved.responsesOptions).toBeNull();
  expect(saved.modelOptions).toBeNull();
  expect(saved.authentication).toBe("bearer");
});

function claudeModels(): Extract<ModelOptions, { kind: "claude" }> {
  return {
    kind: "claude",
    primaryOneM: true,
    haikuModel: "vendor-haiku",
    sonnetModel: "vendor-sonnet",
    sonnetOneM: false,
    opusModel: "vendor-opus",
    opusOneM: true,
    availableModels: ["vendor-main", "vendor-fable", "vendor-worker"],
    fableModel: "vendor-fable",
    fableOneM: true,
    subagentModel: "vendor-worker",
    subagentOneM: true,
    displayNames: {
      haiku: "Fast display name", sonnet: "Sonnet display name",
      opus: "Opus display name", fable: "Fable display name",
    },
  };
}

function profileFor(
  protocol: UpstreamProtocol,
  authentication: AuthenticationScheme | null | undefined,
): ProviderProfile {
  return {
    id: "auth-draft-fixture",
    app: "claude",
    routeMode: "custom",
    name: "Imported Claude provider",
    model: "vendor-main",
    baseUrl: "http://127.0.0.1:9/v1",
    apiKey: "draft-auth-fixture-key",
    ...(authentication === undefined ? {} : { authentication }),
    upstreamProtocol: protocol,
    responsesOptions: protocol === "responses" ? { requestMode: "standard" } : null,
    maxOutputTokens: 8192,
    modelOptions: claudeModels(),
    parameters: { settings: {} },
    notes: "Preserved local note",
    websiteUrl: null,
    usageQuery: null,
  };
}

function savedDraft(draft: ProviderEditorDraft): ProviderDraft {
  const prepared = prepareDraft(draft);
  expect(prepared).not.toBeNull();
  if (!prepared) throw new Error("valid fixture draft must save");
  return JSON.parse(JSON.stringify(prepared)) as ProviderDraft;
}

it.each(connections)("saving $protocol/$authentication preserves imported auth and every Claude model field", ({ protocol, authentication }) => {
  const source = profileFor(protocol, authentication);
  const before = JSON.parse(JSON.stringify(source)) as ProviderProfile;
  const draft = draftFrom(source, "codex");
  const saved = savedDraft(draft);

  expect(saved.authentication).toBe(authentication ?? null);
  expect(saved.app).toBe("claude");
  expect(saved.upstreamProtocol).toBe(protocol);
  expect(saved.modelOptions).toStrictEqual(source.modelOptions);
  expect(saved.parameters).toStrictEqual(source.parameters);
  expect(source).toStrictEqual(before);
});

it.each(connections)("renaming $protocol/$authentication keeps Fable, subagent and display names through another edit", ({ protocol, authentication }) => {
  const source = profileFor(protocol, authentication);
  const renamed = savedDraft({ ...draftFrom(source, "claude"), name: "  Renamed provider  " });
  const reopened = draftFrom({ ...source, ...renamed }, "claude");
  const resaved = savedDraft(reopened);

  expect(resaved.name).toBe("Renamed provider");
  expect(resaved.authentication).toBe(authentication ?? null);
  expect(resaved.modelOptions).toStrictEqual(source.modelOptions);
  expect(resaved.model).toBe(source.model);
  expect(source.name).toBe("Imported Claude provider");
});

it.each(connections)("changing the primary model on $protocol/$authentication keeps independent auth and mappings", ({ protocol, authentication }) => {
  const source = profileFor(protocol, authentication);
  const saved = savedDraft({ ...draftFrom(source, "claude"), model: "  updated-main  " });

  expect(saved.model).toBe("updated-main");
  expect(saved.authentication).toBe(authentication ?? null);
  expect(saved.modelOptions).toStrictEqual(source.modelOptions);
  expect(source.model).toBe("vendor-main");
});

const modelEdits: Array<{ label: string; patch: Partial<ClaudeModelSettings> }> = [
  { label: "primary context", patch: { primaryOneM: false } },
  { label: "Haiku model", patch: { haikuModel: "updated-haiku" } },
  { label: "Sonnet model", patch: { sonnetModel: "updated-sonnet" } },
  { label: "Sonnet context", patch: { sonnetOneM: true } },
  { label: "cleared Sonnet model", patch: { sonnetModel: null, sonnetOneM: false } },
  { label: "Opus context", patch: { opusOneM: false } },
  { label: "available models", patch: { availableModels: ["updated-main", "vendor-fable"] } },
];

it.each(modelEdits)("editing $label preserves non-visible imported model fields and either auth scheme", ({ patch }) => {
  for (const authentication of ["bearer", "xApiKey"] as const) {
    const source = profileFor("anthropicMessages", authentication);
    const before = JSON.parse(JSON.stringify(source)) as ProviderProfile;
    const draft = draftFrom(source, "claude");
    const saved = savedDraft({ ...draft, modelOptions: claudeOptions(draft.modelOptions, patch) });

    expect(saved.authentication).toBe(authentication);
    expect(saved.modelOptions).toStrictEqual({ ...source.modelOptions, ...patch });
    expect(saved.modelOptions).toMatchObject({
      fableModel: "vendor-fable", fableOneM: true,
      subagentModel: "vendor-worker", subagentOneM: true,
      displayNames: claudeModels().displayNames,
    });
    expect(source).toStrictEqual(before);
  }
});

it("preserves explicit null mappings, false context flags and absent display names", () => {
  const source = profileFor("anthropicMessages", "bearer");
  source.modelOptions = {
    ...claudeModels(), fableModel: null, fableOneM: false,
    subagentModel: null, subagentOneM: false, displayNames: null,
  };
  const draft = draftFrom(source, "claude");
  const saved = savedDraft({
    ...draft, modelOptions: claudeOptions(draft.modelOptions, { opusModel: "updated-opus" }),
  });
  expect(saved.authentication).toBe("bearer");
  expect(saved.modelOptions).toStrictEqual({ ...source.modelOptions, opusModel: "updated-opus" });
});

it("a new Claude draft leaves authentication unset instead of persisting an inferred scheme", () => {
  const draft = draftFrom(null, "claude");
  const saved = savedDraft({
    ...draft, name: "New provider", apiKey: "new-fixture-key",
    baseUrl: "http://127.0.0.1:9", parameters: { settings: {} },
  });
  expect(saved.upstreamProtocol).toBe("anthropicMessages");
  expect(saved).not.toHaveProperty("authentication");
  expect(saved.modelOptions).toBeNull();
});

it("preserves imported Haiku 1M mappings and Claude model discovery overrides without changing editor controls", () => {
  const source = profileFor("anthropicMessages", "bearer");
  source.modelOptions = { ...claudeModels(), haikuOneM: true };
  source.connection = { claudeModelsUrl: "https://models.example/v1/models" };
  const saved = savedDraft(draftFrom(source, "claude"));
  expect(saved.modelOptions).toMatchObject({ kind: "claude", haikuOneM: true });
  expect(saved.connection).toEqual(source.connection);
});

it("clears a hidden imported Haiku 1M flag only when the visible model is deliberately changed", () => {
  const current = { ...claudeModels(), haikuOneM: true };
  expect(claudeOptions(current, { haikuModel: null })).toMatchObject({ haikuModel: null, haikuOneM: false });
  expect(claudeOptions(current, { haikuModel: current.haikuModel })).toMatchObject({ haikuOneM: true });
  expect(claudeOptions(current, { sonnetModel: "another-sonnet" })).toMatchObject({ haikuOneM: true });
});
