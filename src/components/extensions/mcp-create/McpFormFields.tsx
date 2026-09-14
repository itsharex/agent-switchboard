import { WandSparkles } from "lucide-react";
import type { AppKind } from "../../../api/client";
import { clientName } from "../../../lib/client-name";
import { Button } from "../../Button";
import { ClientLogo } from "../../ClientLogo";
import { Input } from "../../Input";
import { Select } from "../../Select";
import { Textarea } from "../../Textarea";
import { Tooltip } from "../../Tooltip";
import { CheckIcon } from "../../icons";
import { MANAGEMENT_CLIENTS } from "../client-presentation";
import { MCP_PRESET_OPTIONS } from "./presets";

interface IdentityProps {
  name: string;
  preset: string;
  busy: boolean;
  onNameChange: (name: string) => void;
  onPresetChange: (preset: string) => void;
}

export function McpIdentityFields({ name, preset, busy, onNameChange, onPresetChange }: IdentityProps) {
  return (
    <div className="asb-mcp-identity">
      <label className="asb-field">
        <span>服务名称</span>
        <Input code aria-label="服务名称" value={name} placeholder="docs-search" disabled={busy}
          title="1–64 位字母、数字、下划线或连字符" onChange={(event) => onNameChange(event.target.value)} />
      </label>
      <div className="asb-field">
        <span>预设</span>
        <Select ariaLabel="MCP 常用预设" value={preset} options={MCP_PRESET_OPTIONS}
          disabled={busy} onChange={onPresetChange} />
      </div>
    </div>
  );
}

interface ClientProps {
  clients: AppKind[];
  busy: boolean;
  onToggle: (client: AppKind) => void;
}

export function McpClientFields({ clients, busy, onToggle }: ClientProps) {
  return (
    <div className="asb-mcp-client-field" role="group" aria-label="启用到客户端">
      <span>启用到</span>
      <div className="asb-mcp-clients">
        {MANAGEMENT_CLIENTS.map((client) => {
          const selected = clients.includes(client);
          return (
            <Tooltip key={client} label={`${clientName(client)}：${selected ? "保存后启用" : "不启用"}`}>
              <Button variant="unstyled" className="asb-mcp-client" role="checkbox"
                aria-label={`保存后启用 ${clientName(client)}`} aria-checked={selected}
                data-selected={selected} data-client={client} disabled={busy} onClick={() => onToggle(client)}>
                <ClientLogo app={client} className="asb-mcp-client-logo" />
                <span>{clientName(client)}</span>
                <span className="asb-mcp-client-check" aria-hidden="true">{selected && <CheckIcon />}</span>
              </Button>
            </Tooltip>
          );
        })}
      </div>
    </div>
  );
}

interface JsonProps {
  value: string;
  busy: boolean;
  focus: boolean;
  invalid: boolean;
  onChange: (json: string) => void;
  onWizard: () => void;
}

export function McpJsonField({ value, busy, focus, invalid, onChange, onWizard }: JsonProps) {
  return (
    <div className="asb-mcp-json-field">
      <div className="asb-mcp-field-heading">
        <span>JSON 配置</span>
        <Button variant="secondary" disabled={busy} onClick={onWizard}>
          <WandSparkles size={16} aria-hidden="true" />配置向导
        </Button>
      </div>
      <Textarea code aria-label="MCP JSON 配置" value={value} rows={12} spellCheck={false}
        autoComplete="off" autoFocus={focus} disabled={busy} aria-invalid={invalid}
        placeholder={'{\n  "command": "uvx",\n  "args": ["mcp-server-fetch"]\n}'}
        onChange={(event) => onChange(event.target.value)} />
    </div>
  );
}
