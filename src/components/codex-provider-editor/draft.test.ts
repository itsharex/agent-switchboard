import { describe, expect, it } from "vitest";
import type { CodexProviderRecord, ProviderModel } from "../../api/client";
import {
  DEFAULT_CODEX_CAPABILITIES,
  catalogEntryFromModel,
  codexDraftFrom,
  mergeFetchedCatalog,
  prepareCodexDraft,
  reconcileCodexCapabilities,
  reconcileCodexUpstream,
  validateCodexDraft,
} from "./draft";

function validDraft() {
  const draft = codexDraftFrom(null);
  draft.name = "Relay";
  draft.endpoint = "https://relay.example/v1";
  draft.apiKey = "secret";
  draft.defaultModel = "relay-pro";
  draft.catalog = [{
    id: "relay-pro",
    contextWindow: 128_000,
    maxOutputTokens: 8_192,
    functionTools: true,
    customTools: true,
    toolSearch: true,
    reasoning: true,
    defaultReasoningLevel: "high",
    supportedReasoningLevels: ["none", "high"],
    images: false,
    compact: true,
  }];
  return draft;
}

function record(): CodexProviderRecord {
  const draft = validDraft();
  return {
    profile: {
      id: "0d0a2b8e-1f3a-4c8e-9a2b-3f4c5d6e7f80",
      name: draft.name,
      endpoint: draft.endpoint,
      apiKey: draft.apiKey,
      upstream: draft.upstream,
      routeMode: "direct",
      requestMode: draft.requestMode,
      defaultModel: draft.defaultModel,
      catalog: draft.catalog.map((entry) => ({
        ...entry,
        contextWindow: entry.contextWindow ?? 128_000,
        maxOutputTokens: entry.maxOutputTokens ?? 8_192,
      })),
      modelRoutes: draft.modelRoutes,
      capabilities: draft.capabilities,
    },
    parameters: { settings: {} },
    notes: "note",
    websiteUrl: "https://example.com",
    usageQuery: null,
    fileHash: "hash",
  };
}

describe("codexDraftFrom", () => {
  it("seeds a new draft with the explicit default capability declaration", () => {
    const draft = codexDraftFrom(null);
    expect(draft.upstream).toBe("responses");
    expect(draft.requestMode).toBe("standard");
    expect(draft.catalog).toEqual([]);
    expect(draft.capabilities).toEqual(DEFAULT_CODEX_CAPABILITIES);
    expect(draft.parameters).toBeNull();
  });

  it("deep-copies catalog and routes from an existing record", () => {
    const source = record();
    source.profile.catalog[0].supportedReasoningLevels.push("low");
    const draft = codexDraftFrom(source);
    draft.catalog[0].supportedReasoningLevels.pop();
    expect(source.profile.catalog[0].supportedReasoningLevels).toHaveLength(3);
  });
});

describe("validateCodexDraft", () => {
  it("accepts a complete draft", () => {
    expect(validateCodexDraft(validDraft())).toEqual([]);
  });

  it("requires identity and a key without inventing a vendor API prefix", () => {
    const draft = validDraft();
    draft.name = " ";
    draft.apiKey = " ";
    draft.endpoint = "https://relay.example";
    const problems = validateCodexDraft(draft);
    expect(problems).toContain("供应商名称不能为空");
    expect(problems).toContain("API 密钥不能为空");
    expect(problems).toHaveLength(2);
  });

  it.each(["responses", "chatCompletions", "anthropicMessages"] as const)("allows a declared service root for %s", (upstream) => {
    const draft = validDraft();
    draft.upstream = upstream;
    draft.endpoint = "https://relay.example";
    expect(validateCodexDraft(draft)).toEqual([]);
  });

  it("requires a non-empty catalog containing the default model", () => {
    const draft = validDraft();
    draft.catalog = [];
    expect(validateCodexDraft(draft)).toContain("模型目录不能为空；请获取模型或手动添加");
    const other = validDraft();
    other.defaultModel = "missing";
    expect(validateCodexDraft(other)).toContain("默认模型必须存在于模型目录");
  });

  it("rejects catalog entries that exceed the declared capabilities", () => {
    const draft = validDraft();
    draft.capabilities.toolSearch = false;
    expect(validateCodexDraft(draft)).toContain("模型工具搜索超出供应商能力：relay-pro");
  });

  it("rejects reasoning level declarations that are missing or inconsistent", () => {
    const draft = validDraft();
    draft.catalog[0].supportedReasoningLevels = [];
    draft.catalog[0].defaultReasoningLevel = "low";
    expect(validateCodexDraft(draft)).toContain("模型未声明推理档位：relay-pro");
    expect(validateCodexDraft(draft)).toContain("模型默认推理档位未声明：relay-pro");
    const nonReasoning = validDraft();
    nonReasoning.catalog[0].reasoning = false;
    nonReasoning.catalog[0].defaultReasoningLevel = "high";
    expect(validateCodexDraft(nonReasoning)).toContain("不支持推理的模型只能声明“无”档位：relay-pro");
  });

  it("rejects chat reasoning without reasoning or outside Chat Completions", () => {
    const chatReasoning = { kind: "configured" as const, thinkingParameter: "none" as const,
      effortParameter: "none" as const, effortMode: "passthrough" as const };
    const noReasoning = validDraft();
    noReasoning.capabilities.reasoning = false;
    noReasoning.capabilities.chatReasoning = chatReasoning;
    expect(validateCodexDraft(noReasoning)).toContain("未启用推理时不能配置 Chat 推理参数");
    const wrongUpstream = validDraft();
    wrongUpstream.capabilities.chatReasoning = chatReasoning;
    expect(validateCodexDraft(wrongUpstream)).toContain("仅 Chat Completions 上游可以配置 Chat 推理参数");
  });

  it("rejects routes that reference models outside the catalog or repeat a client model", () => {
    const draft = validDraft();
    draft.modelRoutes = [
      { clientModel: "relay-pro", upstreamModel: "vendor-pro" },
      { clientModel: "relay-pro", upstreamModel: "vendor-pro-2" },
      { clientModel: "missing", upstreamModel: "vendor-x" },
    ];
    const problems = validateCodexDraft(draft);
    expect(problems).toContain("模型映射含有重复客户端模型：relay-pro");
    expect(problems).toContain("模型映射引用了目录外客户端模型：missing");
  });
});

describe("catalog generation from fetched models", () => {
  const models: ProviderModel[] = [
    { id: "relay-pro", ownedBy: "relay" },
    { id: "relay-mini", ownedBy: "relay" },
  ];

  it("narrows generated rows to the declared capabilities", () => {
    const capabilities = { ...DEFAULT_CODEX_CAPABILITIES, toolSearch: false };
    const entry = catalogEntryFromModel(models[0], capabilities);
    expect(entry.toolSearch).toBe(false);
    expect(entry.reasoning).toBe(true);
    expect(entry.defaultReasoningLevel).toBe("medium");
    expect(entry.supportedReasoningLevels).toEqual(["none", "minimal", "low", "medium", "high", "xhigh", "max", "ultra"]);
    expect(entry.contextWindow).toBeNull();
    expect(entry.maxOutputTokens).toBeNull();
  });

  it("declares only the none level when reasoning is off", () => {
    const capabilities = { ...DEFAULT_CODEX_CAPABILITIES, reasoning: false };
    const entry = catalogEntryFromModel(models[1], capabilities);
    expect(entry.reasoning).toBe(false);
    expect(entry.defaultReasoningLevel).toBe("none");
    expect(entry.supportedReasoningLevels).toEqual(["none"]);
  });

  it("prefills official published limits for official model ids", () => {
    const entry = catalogEntryFromModel({ id: "gpt-6-astra", ownedBy: "openai" }, DEFAULT_CODEX_CAPABILITIES);
    expect(entry.contextWindow).toBe(1_050_000);
    expect(entry.maxOutputTokens).toBe(128_000);
  });

  it("keeps a source-stated limit above the official table", () => {
    const entry = catalogEntryFromModel(
      { id: "gpt-5.6-terra", ownedBy: null },
      DEFAULT_CODEX_CAPABILITIES,
      { contextWindow: 272_000 },
    );
    expect(entry.contextWindow).toBe(272_000);
  });

  it("merges fetched models without overwriting existing rows", () => {
    const draft = validDraft();
    const merged = mergeFetchedCatalog(draft.catalog, models, draft.capabilities);
    expect(merged.map((entry) => entry.id)).toEqual(["relay-pro", "relay-mini"]);
    expect(merged[0].defaultReasoningLevel).toBe("high");
  });
});

describe("capability and upstream reconciliation", () => {
  it("strips catalog flags and reasoning when a capability is withdrawn", () => {
    const draft = validDraft();
    const next = reconcileCodexCapabilities(draft, { ...draft.capabilities, toolSearch: false, reasoning: false });
    expect(next.capabilities.chatReasoning).toEqual({ kind: "unsupported" });
    expect(next.catalog[0].toolSearch).toBe(false);
    expect(next.catalog[0].reasoning).toBe(false);
    expect(next.catalog[0].supportedReasoningLevels).toEqual(["none"]);
    expect(next.catalog[0].defaultReasoningLevel).toBe("none");
    expect(validateCodexDraft(next)).toEqual([]);
  });

  it("drops chat reasoning when the upstream leaves Chat Completions", () => {
    const draft = validDraft();
    draft.upstream = "chatCompletions";
    draft.capabilities.chatReasoning = { kind: "configured", thinkingParameter: "thinking",
      effortParameter: "reasoningEffort", effortMode: "passthrough" };
    expect(validateCodexDraft(draft)).toEqual([]);
    const next = reconcileCodexUpstream(draft, "responses");
    expect(next.capabilities.chatReasoning).toEqual({ kind: "unsupported" });
    expect(validateCodexDraft(next)).toEqual([]);
  });
});

describe("prepareCodexDraft", () => {
  it("trims identity fields and keeps the strict contract shape", () => {
    const draft = validDraft();
    draft.parameters = { settings: {} };
    draft.name = "  Relay  ";
    draft.websiteUrl = "  ";
    draft.notes = "";
    const prepared = prepareCodexDraft(draft);
    expect(prepared).not.toBeNull();
    expect(prepared!.name).toBe("Relay");
    expect(prepared!.websiteUrl).toBeNull();
    expect(prepared!.notes).toBeNull();
    expect(prepared!.upstream).toBe("responses");
    expect(prepared!.catalog).toEqual(draft.catalog);
  });

  it("materializes empty limits with the generic defaults for unknown models", () => {
    const draft = validDraft();
    draft.parameters = { settings: {} };
    draft.catalog[0].contextWindow = null;
    draft.catalog[0].maxOutputTokens = null;
    const prepared = prepareCodexDraft(draft)!;
    expect(prepared.catalog[0].contextWindow).toBe(128_000);
    expect(prepared.catalog[0].maxOutputTokens).toBe(8_192);
  });

  it("materializes empty limits with the official numbers for official models", () => {
    const draft = validDraft();
    draft.parameters = { settings: {} };
    draft.catalog[0].id = "gpt-6-astra";
    draft.catalog[0].contextWindow = null;
    draft.catalog[0].maxOutputTokens = null;
    const prepared = prepareCodexDraft(draft)!;
    expect(prepared.catalog[0].contextWindow).toBe(1_050_000);
    expect(prepared.catalog[0].maxOutputTokens).toBe(128_000);
  });

  it("refuses to build a wire draft before parameters are seeded", () => {
    expect(prepareCodexDraft(codexDraftFrom(null))).toBeNull();
  });
});

