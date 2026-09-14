import { expect, it } from "vitest";
import { buildMetadata, metadataDraft } from "./metadata";

it("omits empty metadata and normalizes optional values", () => {
  expect(buildMetadata(metadataDraft())).toBeUndefined();
  expect(buildMetadata({ ...metadataDraft(), displayName: " Docs ", tags: "docs, search, docs, " }))
    .toEqual({ displayName: "Docs", tags: ["docs", "search"] });
});

it.each([
  { displayName: "x".repeat(129) }, { description: "x".repeat(2001) }, { tags: "x".repeat(65) },
  { tags: Array.from({ length: 33 }, (_, index) => "tag-" + index).join(",") },
  { tags: "bad\nlabel" }, { displayName: "a\0b" }, { homepage: "file:///private" },
  { docs: "https://user:password@example.test" }, { docs: "https://example.test/line\nbreak" },
])("rejects invalid metadata %j", (patch) => {
  expect(() => buildMetadata({ ...metadataDraft(), ...patch })).toThrow();
});
