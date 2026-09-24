import { useI18n } from "../../../i18n";
import type { CodexServerOptions } from "../../../api/client";
import { Input } from "../../Input";
import { Select } from "../../Select";

export function McpCodexOptions({ value, busy, onChange }: {
  value?: CodexServerOptions; busy: boolean; onChange: (value: CodexServerOptions | undefined) => void;
}) {
  const { t } = useI18n();
  const update = (key: keyof CodexServerOptions, next: string | number | boolean | undefined) => {
    const options = Object.fromEntries(Object.entries({ ...value, [key]: next }).filter(([, entry]) => entry != null));
    onChange(Object.keys(options).length ? options : undefined);
  };
  return <div className="asb-mcp-wizard-section" role="group" aria-label={t("mcp.codex.title")}>
    <div className="asb-mcp-field-heading"><span>{t("mcp.codex.title")}</span></div>
    <div className="asb-mcp-metadata-fields">
      <label className="asb-field asb-mcp-description"><span>{t("mcp.codex.cwd")}</span>
        <Input code aria-label={t("mcp.codex.cwd")} value={value?.cwd ?? ""} placeholder={t("mcp.placeholder.optional")} disabled={busy}
          onChange={(event) => update("cwd", event.target.value || undefined)} />
      </label>
      {([["startupTimeoutSec", "mcp.codex.startupTimeout"], ["toolTimeoutSec", "mcp.codex.toolTimeout"]] as const).map(([key, labelKey]) =>
        <label className="asb-field" key={key}><span>{t(labelKey)}</span>
          <Input type="number" aria-label={t(labelKey)} min={0} step={1} max={Number.MAX_SAFE_INTEGER}
            value={value?.[key] ?? ""} disabled={busy}
            onChange={(event) => update(key, event.target.value === "" ? undefined : Number(event.target.value))} />
        </label>)}
      <div className="asb-field"><span>{t("mcp.codex.required")}</span>
        <Select ariaLabel="Codex required" value={value?.required == null ? "" : String(value.required)} disabled={busy}
          options={[{ value: "", label: t("mcp.option.unset") }, { value: "true", label: t("mcp.option.yes") }, { value: "false", label: t("mcp.option.no") }]}
          onChange={(next) => update("required", next === "" ? undefined : next === "true")} />
      </div>
    </div>
  </div>;
}
