import { useState } from "react";
import { Input } from "../../Input";
import { Textarea } from "../../Textarea";
import type { MetadataDraft } from "./metadata";

export function McpMetadataFields({ value, busy, onChange }: {
  value: MetadataDraft; busy: boolean; onChange: (value: MetadataDraft) => void;
}) {
  const [expanded, setExpanded] = useState(Boolean(value.description || value.tags || value.homepage || value.docs));
  const change = (key: keyof MetadataDraft, text: string) => onChange({ ...value, [key]: text });
  return <>
    <label className="asb-field"><span>显示名称</span>
      <Input aria-label="显示名称" value={value.displayName} disabled={busy} placeholder="可选"
        onChange={(event) => change("displayName", event.target.value)} />
    </label>
    <details className="asb-mcp-metadata" open={expanded} onToggle={(event) => setExpanded(event.currentTarget.open)}>
      <summary>附加信息</summary>
      <div className="asb-mcp-metadata-fields">
        <label className="asb-field asb-mcp-description"><span>描述</span>
          <Textarea aria-label="描述" value={value.description} rows={2} disabled={busy}
            onChange={(event) => change("description", event.target.value)} />
        </label>
        <label className="asb-field asb-mcp-description"><span>标签</span>
          <Input aria-label="标签" value={value.tags} placeholder="docs, search" disabled={busy}
            onChange={(event) => change("tags", event.target.value)} />
        </label>
        {([["homepage", "主页"], ["docs", "文档链接"]] as const).map(([key, label]) =>
          <label className="asb-field" key={key}><span>{label}</span>
            <Input code aria-label={label} value={value[key]} disabled={busy} placeholder="https://"
              onChange={(event) => change(key, event.target.value)} />
          </label>)}
      </div>
    </details>
  </>;
}
