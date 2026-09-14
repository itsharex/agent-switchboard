import { expect, it } from "vitest";
import { authenticationDraft } from "./ProviderAuthentication";
import { codexProfiles } from "../../test/app-fixtures";
it("changes only Codex authentication while preserving all model, connection and visual-setting facts", () => {
  const source = structuredClone(codexProfiles[0]);
  source.profile.catalog[0].displayName = "模型展示名";
  source.profile.catalog[0].baseInstructions = "Keep this model instruction.";
  source.profile.connection = { customUserAgent: "isolated-agent", isFullUrl: false };
  const before = structuredClone(source);
  const draft = authenticationDraft(source, "xApiKey");
  expect(draft.authentication).toBe("xApiKey");
  expect(draft.catalog).toEqual(before.profile.catalog);
  expect(draft.connection).toEqual(before.profile.connection);
  expect(draft.parameters).toEqual(before.parameters);
  expect(draft.usageQuery).toEqual(before.usageQuery);
  expect(draft).not.toHaveProperty("id"); expect(draft).not.toHaveProperty("routeMode");
  expect(source).toEqual(before);
});
