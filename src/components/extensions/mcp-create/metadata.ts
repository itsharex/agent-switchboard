import { uiMessage } from "../../../i18n/errors";
import type { McpMetadata } from "../../../api/client";

export interface MetadataDraft {
  displayName: string;
  description: string;
  tags: string;
  homepage: string;
  docs: string;
}

export function metadataDraft(value?: McpMetadata | null): MetadataDraft {
  return { displayName: value?.displayName ?? "", description: value?.description ?? "",
    tags: value?.tags?.join(", ") ?? "", homepage: value?.homepage ?? "", docs: value?.docs ?? "" };
}

export function buildMetadata(draft: MetadataDraft): McpMetadata | undefined {
  const result: McpMetadata = {};
  for (const [key, labelKey, limit] of [["displayName", "mcp.field.displayName", 128], ["description", "mcp.field.description", 2000]] as const) {
    const value = draft[key].trim();
    if (!value) continue;
    if ([...value].length > limit || value.includes("\0")) throw uiMessage("mcp.error.metadataTooLong", { labelKey });
    result[key] = value;
  }
  const tags = [...new Set(draft.tags.split(",").map((tag) => tag.trim()).filter(Boolean))];
  if (tags.length > 32 || tags.some((tag) => [...tag].length > 64 || /[\u0000-\u001f\u007f-\u009f]/.test(tag))) {
    throw uiMessage("mcp.error.tagsInvalid");
  }
  if (tags.length) result.tags = tags;
  for (const [key, labelKey] of [["homepage", "mcp.field.homepage"], ["docs", "mcp.field.docs"]] as const) {
    const value = draft[key].trim();
    if (!value) continue;
    let url: URL;
    try { url = new URL(value); } catch { throw uiMessage("mcp.error.urlHttpOnly", { labelKey }); }
    if (new TextEncoder().encode(value).length > 2048 || /[\u0000-\u001f\u007f-\u009f]/.test(value)
      || !["http:", "https:"].includes(url.protocol) || !url.hostname || url.username || url.password) {
      throw uiMessage("mcp.error.urlCredentials", { labelKey });
    }
    result[key] = value;
  }
  return Object.keys(result).length ? result : undefined;
}

export function metadataEqual(a?: McpMetadata | null, b?: McpMetadata | null): boolean {
  return JSON.stringify(metadataDraft(a)) === JSON.stringify(metadataDraft(b));
}
