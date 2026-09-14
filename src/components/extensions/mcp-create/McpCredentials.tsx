import { useRef } from "react";
import { Button } from "../../Button";
import { Checkbox } from "../../Checkbox";
import { Input } from "../../Input";
import { Select } from "../../Select";
import { Textarea } from "../../Textarea";
import { Tooltip } from "../../Tooltip";
import { PlusIcon, TrashIcon } from "../../icons";
import type { CreateCredential } from "./mcp-json";
import { credentialText, credentialValue, type CredentialRow } from "./wizard-state";

const VALUE_MODES = [
  { value: "plain", label: "明文值" },
  { value: "envRef", label: "环境变量" },
  { value: "secret", label: "系统凭据" },
];

interface ValueProps {
  value: CreateCredential;
  label: string;
  busy: boolean;
  canKeep?: boolean;
  onChange: (value: CreateCredential) => void;
}

function CredentialFields({ value, label, busy, canKeep, onChange }: ValueProps) {
  const text = credentialText(value);
  const inputProps = {
    "aria-label": `${label}值`, value: text, disabled: busy, autoComplete: "off",
    onChange: (event: { target: { value: string } }) => onChange(credentialValue(value.mode, event.target.value)),
  };
  return (
    <>
      <div className="asb-mcp-slot-kind">
        <Select ariaLabel={`${label}值类型`} value={value.mode} disabled={busy}
          options={canKeep ? [{ value: "secretConfigured", label: "保持已存凭据" }, ...VALUE_MODES] : VALUE_MODES}
          onChange={(mode) => onChange(credentialValue(mode as CreateCredential["mode"], text))} />
      </div>
      <div className="asb-mcp-slot-value">
        {value.mode === "secretConfigured" ? <span className="asb-scope-note">已设置凭据（保持不变）</span>
          : value.mode === "envRef" ? <Input code {...inputProps} placeholder="环境变量名" />
          : value.mode === "secret" && !/[\r\n]/.test(text) ? <Input {...inputProps} type="password" placeholder="新凭据" />
            : <Textarea code rows={1} {...inputProps} placeholder="值" />}
      </div>
    </>
  );
}

interface RowsProps {
  rows: CredentialRow[];
  label: "环境变量" | "请求头";
  busy: boolean;
  onChange: (rows: CredentialRow[]) => void;
}

export function McpCredentialRows({ rows, label, busy, onChange }: RowsProps) {
  const nextId = useRef(Math.max(0, ...rows.map(({ id }) => id)) + 1);
  return (
    <div className="asb-mcp-wizard-section" role="group" aria-label={label}>
      <div className="asb-mcp-field-heading">
        <span>{label}</span>
        <Tooltip label={`添加${label}`}>
          <Button variant="icon" disabled={busy} aria-label={`添加${label}`}
            onClick={() => onChange([...rows, { id: nextId.current++, name: "", value: { mode: "plain", value: "" } }])}>
            <PlusIcon />
          </Button>
        </Tooltip>
      </div>
      {rows.map((row, index) => (
        <div className="asb-mcp-credential-row" key={row.id}>
          <div className="asb-mcp-slot-name">
            <Input code aria-label={`${label}名 ${index + 1}`} placeholder={label === "环境变量" ? "变量名" : "Header 名"}
              value={row.name} disabled={busy || row.value.mode === "secretConfigured"}
              onChange={(event) => onChange(rows.map((item) => item.id === row.id ? { ...item, name: event.target.value } : item))} />
          </div>
          <CredentialFields label={`${label} ${index + 1} `} value={row.value} busy={busy} canKeep={row.storedName === row.name}
            onChange={(value) => onChange(rows.map((item) => item.id === row.id ? { ...item, value } : item))} />
          <div className="asb-mcp-slot-remove">
            <Tooltip label={`移除${label} ${index + 1}`}>
              <Button variant="icon" disabled={busy} aria-label={`移除${label} ${index + 1}`}
                onClick={() => onChange(rows.filter(({ id }) => id !== row.id))}><TrashIcon /></Button>
            </Tooltip>
          </div>
        </div>
      ))}
    </div>
  );
}

export function McpBearerField({ value, busy, canKeep, onChange }: {
  value?: CreateCredential; busy: boolean; canKeep?: boolean; onChange: (value: CreateCredential | undefined) => void;
}) {
  return (
    <div className="asb-mcp-wizard-section">
      <Checkbox label="Bearer 凭据" checked={value !== undefined} disabled={busy}
        onChange={(checked) => onChange(checked ? canKeep ? { mode: "secretConfigured" } : { mode: "envRef", name: "" } : undefined)} />
      {value && <div className="asb-mcp-bearer-row"><CredentialFields value={value} label="Bearer 凭据"
        busy={busy} canKeep={canKeep} onChange={onChange} /></div>}
    </div>
  );
}
