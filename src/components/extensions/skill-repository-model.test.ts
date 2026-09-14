import { expect, it } from "vitest";
import { skillRepositoryLabel, skillRepositoryName, skillSourceUrl } from "./skill-repository-model";
import { sourceErrorMessage } from "./skill-source-model";

it("normalizes repository identities without accepting unrelated or credential-bearing URLs", () => {
  expect(skillRepositoryName(" example/skills ")).toBe("example/skills");
  expect(skillRepositoryName("https://github.com/example/skills.git/")).toBe("example/skills");
  for (const value of [
    "https://user:private-value@github.com/example/skills", "https://github.com/example/skills?token=private-value",
    "https://github.com.evil.test/example/skills", "file:///example/skills", "owner/repo/tree/main", "owner/..",
  ]) expect(skillRepositoryName(value)).toBeNull();
});

it("only links to known HTTPS sources and discards credentials and query strings", () => {
  expect(skillSourceUrl("example/skills", "https://github.com/example/skills/blob/main/SKILL.md"))
    .toBe("https://github.com/example/skills/blob/main/SKILL.md");
  for (const value of [
    "javascript:alert(1)", "https://user:private-value@github.com/example/skills", "https://example.test/skills",
    "https://skills.sh/example/skills?token=private-value", "https://github.com:8080/example/skills",
  ]) expect(skillSourceUrl("example/skills", value)).toBe("https://github.com/example/skills");
  expect(skillSourceUrl("https://private-value@github.com/example/skills")).toBeNull();
  expect(skillRepositoryLabel({ repo: "https://private-value@github.com/example/skills", subpath: "" }))
    .not.toContain("private-value");
});

it("keeps actionable source failures while hiding credential URL material", () => {
  const message = sourceErrorMessage({ message:
    "HTTP 401: https://user:private-value@github.com/example/skills?token=other-private-value" }, "读取失败");
  expect(message).toBe("HTTP 401: https://github.com/example/skills");
  expect(message).not.toContain("private-value");
  expect(sourceErrorMessage(new Error("请求超时"), "读取失败")).toBe("请求超时");
  expect(sourceErrorMessage(null, "读取失败")).toBe("读取失败");
});
