// Rebuild only from the pinned local CC Switch checkout. No downloads or client-file writes.
import fs from "node:fs";
import path from "node:path";
import vm from "node:vm";
import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import ts from "typescript";

const revision = "d695a2d77fd9081eafd3e9eedcbf2a97b3410928";
const checkout = process.argv[2];
if (!checkout) throw new Error("Usage: node scripts/import-claude-presets.mjs <cc-switch-checkout>");
const source = "src/config/claudeProviderPresets.ts";
const head = execFileSync("git", ["-C", checkout, "rev-parse", "HEAD"], { encoding: "utf8" }).trim();
if (head !== revision) throw new Error("Claude preset source revision does not match the pinned baseline");
execFileSync("git", ["-C", checkout, "diff", "--exit-code", "HEAD", "--", source], { stdio: "pipe" });
const input = fs.readFileSync(path.join(checkout, source), "utf8");
const compiled = ts.transpileModule(input, { compilerOptions: {
  target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS,
} }).outputText;
const context = { exports: {}, require: (name) => { throw new Error("Unexpected preset runtime import: " + name); } };
vm.runInNewContext(compiled, context, { timeout: 1_000 });
const fields = ["name", "websiteUrl", "apiKeyUrl", "settingsConfig", "isOfficial", "category", "apiKeyField",
  "templateValues", "endpointCandidates", "apiFormat", "providerType", "requiresOAuth", "hidden", "modelsUrl"];
const cleanNavigation = (value) => {
  if (!value) return value;
  const url = new URL(value);
  for (const key of [...url.searchParams.keys()]) {
    if (key === "aff" || key === "ref" || key.startsWith("utm_")) url.searchParams.delete(key);
  }
  return url.href;
};
const presets = context.exports.providerPresets.map((preset, index) => ({
  id: "claude-preset-" + String(index + 1).padStart(2, "0"),
  ...Object.fromEntries(fields.map((field) => [field, preset[field]])),
  websiteUrl: cleanNavigation(preset.websiteUrl), apiKeyUrl: cleanNavigation(preset.apiKeyUrl),
}));
if (presets.length !== 90) throw new Error("Pinned Claude preset coverage unexpectedly changed");
const output = fileURLToPath(new URL("../crates/asb-core/src/claude_presets/catalog.json", import.meta.url));
fs.mkdirSync(path.dirname(output), { recursive: true });
fs.writeFileSync(output, JSON.stringify({ source: "farion1231/cc-switch", revision, presets }, null, 2) + "\n");
console.log("Generated " + presets.length + " offline Claude presets: " + output);
console.log("Formats: " + [...new Set(presets.map((preset) => preset.apiFormat ?? "anthropic"))].join(", "));
console.log("Template variables: " + [...new Set(presets.flatMap((preset) => Object.keys(preset.templateValues ?? {})))].join(", "));
