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
  for (const [key, label, limit] of [["displayName", "显示名称", 128], ["description", "描述", 2000]] as const) {
    const value = draft[key].trim();
    if (!value) continue;
    if ([...value].length > limit || value.includes("\0")) throw new Error(label + "过长或包含空字符");
    result[key] = value;
  }
  const tags = [...new Set(draft.tags.split(",").map((tag) => tag.trim()).filter(Boolean))];
  if (tags.length > 32 || tags.some((tag) => [...tag].length > 64 || /[\u0000-\u001f\u007f-\u009f]/.test(tag))) {
    throw new Error("标签最多 32 项，每项最多 64 个字符且不含控制字符");
  }
  if (tags.length) result.tags = tags;
  for (const [key, label] of [["homepage", "主页"], ["docs", "文档链接"]] as const) {
    const value = draft[key].trim();
    if (!value) continue;
    let url: URL;
    try { url = new URL(value); } catch { throw new Error(label + "须为有效的 HTTP / HTTPS URL"); }
    if (new TextEncoder().encode(value).length > 2048 || /[\u0000-\u001f\u007f-\u009f]/.test(value)
      || !["http:", "https:"].includes(url.protocol) || !url.hostname || url.username || url.password) {
      throw new Error(label + "须为有效的 HTTP / HTTPS URL，且不能含登录凭据");
    }
    result[key] = value;
  }
  return Object.keys(result).length ? result : undefined;
}

export function metadataEqual(a?: McpMetadata | null, b?: McpMetadata | null): boolean {
  return JSON.stringify(metadataDraft(a)) === JSON.stringify(metadataDraft(b));
}
