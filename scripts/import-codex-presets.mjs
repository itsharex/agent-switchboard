// Rebuild the pinned offline data from an explicitly supplied clean the source application checkout.
// No network access or provider/credential/configuration writes are performed.
import fs from "node:fs";
import path from "node:path";
import vm from "node:vm";
import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import ts from "typescript";

const revision = "d695a2d77fd9081eafd3e9eedcbf2a97b3410928";
const checkout = process.argv[2];
if (!checkout) throw new Error("Usage: node scripts/import-codex-presets.mjs <source-checkout>");
const source = "src/config/codexProviderPresets.ts";
const head = execFileSync("git", ["-C", checkout, "rev-parse", "HEAD"], { encoding: "utf8" }).trim();
if (head !== revision) throw new Error("preset reference revision does not match the pinned baseline");
execFileSync("git", ["-C", checkout, "diff", "--exit-code", "HEAD", "--", source], { stdio: "pipe" });
const input = fs.readFileSync(path.join(checkout, source), "utf8");
const compiled = ts.transpileModule(input, { compilerOptions: {
  target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS,
} }).outputText;
const context = { exports: {}, require: (name) => { throw new Error("Unexpected preset runtime import: " + name); } };
vm.runInNewContext(compiled, context, { timeout: 1_000 });
const fields = ["name", "websiteUrl", "apiKeyUrl", "auth", "config", "isOfficial", "isCustomTemplate",
  "endpointCandidates", "apiFormat", "providerType", "requiresOAuth", "modelCatalog", "codexChatReasoning",
  "promptCacheRouting", "category"];
const presets = context.exports.codexProviderPresets.map((preset, index) => ({
  id: "codex-preset-" + String(index + 1).padStart(2, "0"),
  ...Object.fromEntries(fields.map((field) => [field, preset[field]])),
}));
if (presets.length !== 85) throw new Error("Pinned Codex preset coverage unexpectedly changed");
const output = fileURLToPath(new URL("../crates/asb-core/src/codex_presets/catalog.json", import.meta.url));
fs.writeFileSync(output, JSON.stringify({ source: "offline-reference", revision, presets }, null, 2) + "\n");
console.log("Generated " + presets.length + " offline Codex presets: " + output);
