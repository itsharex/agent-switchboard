import { useState } from "react";
import { useI18n } from "../../../i18n";
import { Input } from "../../Input";
import { Textarea } from "../../Textarea";
import type { MetadataDraft } from "./metadata";

export function McpMetadataFields({ value, busy, onChange }: {
  value: MetadataDraft; busy: boolean; onChange: (value: MetadataDraft) => void;
}) {
  const { t } = useI18n();
  const [expanded, setExpanded] = useState(Boolean(value.description || value.tags || value.homepage || value.docs));
  const change = (key: keyof MetadataDraft, text: string) => onChange({ ...value, [key]: text });
  return <>
    <label className="asb-field"><span>{t("mcp.field.displayName")}</span>
      <Input aria-label={t("mcp.field.displayName")} value={value.displayName} disabled={busy} placeholder={t("mcp.placeholder.optional")}
        onChange={(event) => change("displayName", event.target.value)} />
    </label>
    <details className="asb-mcp-metadata" open={expanded} onToggle={(event) => setExpanded(event.currentTarget.open)}>
      <summary>{t("mcp.field.metadataSummary")}</summary>
      <div className="asb-mcp-metadata-fields">
        <label className="asb-field asb-mcp-description"><span>{t("mcp.field.description")}</span>
          <Textarea aria-label={t("mcp.field.description")} value={value.description} rows={2} disabled={busy}
            onChange={(event) => change("description", event.target.value)} />
        </label>
        <label className="asb-field asb-mcp-description"><span>{t("mcp.field.tags")}</span>
          <Input aria-label={t("mcp.field.tags")} value={value.tags} placeholder="docs, search" disabled={busy}
            onChange={(event) => change("tags", event.target.value)} />
        </label>
        {([["homepage", "mcp.field.homepage"], ["docs", "mcp.field.docs"]] as const).map(([key, labelKey]) =>
          <label className="asb-field" key={key}><span>{t(labelKey)}</span>
            <Input code aria-label={t(labelKey)} value={value[key]} disabled={busy} placeholder="https://"
              onChange={(event) => change(key, event.target.value)} />
          </label>)}
      </div>
    </details>
  </>;
}
