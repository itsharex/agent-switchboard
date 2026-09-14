import type { CodexServerOptions } from "../../../api/client";
import { Input } from "../../Input";
import { Select } from "../../Select";

export function McpCodexOptions({ value, busy, onChange }: {
  value?: CodexServerOptions; busy: boolean; onChange: (value: CodexServerOptions | undefined) => void;
}) {
  const update = (key: keyof CodexServerOptions, next: string | number | boolean | undefined) => {
    const options = Object.fromEntries(Object.entries({ ...value, [key]: next }).filter(([, entry]) => entry != null));
    onChange(Object.keys(options).length ? options : undefined);
  };
  return <div className="asb-mcp-wizard-section" role="group" aria-label="Codex 选项">
    <div className="asb-mcp-field-heading"><span>Codex 选项</span></div>
    <div className="asb-mcp-metadata-fields">
      <label className="asb-field asb-mcp-description"><span>工作目录</span>
        <Input code aria-label="工作目录" value={value?.cwd ?? ""} placeholder="可选" disabled={busy}
          onChange={(event) => update("cwd", event.target.value || undefined)} />
      </label>
      {([["startupTimeoutSec", "启动超时（秒）"], ["toolTimeoutSec", "工具超时（秒）"]] as const).map(([key, label]) =>
        <label className="asb-field" key={key}><span>{label}</span>
          <Input type="number" aria-label={label} min={0} step={1} max={Number.MAX_SAFE_INTEGER}
            value={value?.[key] ?? ""} disabled={busy}
            onChange={(event) => update(key, event.target.value === "" ? undefined : Number(event.target.value))} />
        </label>)}
      <div className="asb-field"><span>必需服务</span>
        <Select ariaLabel="Codex required" value={value?.required == null ? "" : String(value.required)} disabled={busy}
          options={[{ value: "", label: "未设置" }, { value: "true", label: "是" }, { value: "false", label: "否" }]}
          onChange={(next) => update("required", next === "" ? undefined : next === "true")} />
      </div>
    </div>
  </div>;
}
