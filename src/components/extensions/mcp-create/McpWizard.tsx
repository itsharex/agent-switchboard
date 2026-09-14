import { useState } from "react";
import { Button } from "../../Button";
import { Input } from "../../Input";
import { Select } from "../../Select";
import { CheckIcon, CloseIcon } from "../../icons";
import { McpArguments } from "./McpArguments";
import { McpCodexOptions } from "./McpCodexOptions";
import { McpBearerField, McpCredentialRows } from "./McpCredentials";
import { validateMcpSource } from "./mcp-draft";
import type { CreateSource, McpType } from "./mcp-json";
import { changeTransport, initializeWizard, transportDropsFields, wizardSource, type WizardServer } from "./wizard-state";

const TRANSPORTS = [
  { value: "stdio", label: "stdio · 本地命令" },
  { value: "http", label: "HTTP · 远程端点" },
  { value: "sse", label: "SSE · 仅 Claude" },
  { value: "ws", label: "WebSocket · 仅 Claude" },
];

function ServerFields({ server, busy, original, onChange }: {
  server: WizardServer; busy: boolean; original?: CreateSource; onChange: (server: WizardServer) => void;
}) {
  if (server.type === "stdio") {
    return (
      <>
        <label className="asb-field"><span>启动命令</span>
          <Input code aria-label="启动命令" value={server.command} disabled={busy} placeholder="npx"
            onChange={(event) => onChange({ ...server, command: event.target.value })} />
        </label>
        <McpArguments rows={server.args} busy={busy} onChange={(args) => onChange({ ...server, args })} />
        <McpCredentialRows label="环境变量" rows={server.env} busy={busy}
          onChange={(env) => onChange({ ...server, env })} />
        <McpCodexOptions value={server.codexOptions} busy={busy}
          onChange={(codexOptions) => onChange({ ...server, codexOptions })} />
      </>
    );
  }
  return (
    <>
      <label className="asb-field"><span>服务地址</span>
        <Input code aria-label="服务地址" value={server.url} disabled={busy} placeholder="https://mcp.example.com"
          onChange={(event) => onChange({ ...server, url: event.target.value })} />
      </label>
      <McpCredentialRows label="请求头" rows={server.headers} busy={busy}
        onChange={(headers) => onChange({ ...server, headers })} />
      {server.type === "http" && <McpBearerField value={server.bearer} busy={busy}
        canKeep={original?.server.type === "http" && original.server.bearer?.mode === "secretConfigured"}
        onChange={(bearer) => onChange({ ...server, bearer })} />}
    </>
  );
}

interface Props {
  initial: CreateSource;
  original?: CreateSource;
  busy: boolean;
  onApply: (source: CreateSource) => void;
  onCancel: () => void;
}

export function McpWizard({ initial, original, busy, onApply, onCancel }: Props) {
  const [name, setName] = useState(initial.name);
  const [server, setServer] = useState(() => initializeWizard(initial.server));
  const [pendingType, setPendingType] = useState<McpType | null>(null);
  const [error, setError] = useState<string | null>(null);
  const update = (next: WizardServer) => { setServer(next); setError(null); };
  const chooseTransport = (value: string) => {
    const type = value as McpType;
    if (transportDropsFields(server, type)) setPendingType(type);
    else { update(changeTransport(server, type)); setPendingType(null); }
  };
  const apply = () => {
    if (busy || pendingType) return;
    try {
      const source = wizardSource(name, server);
      validateMcpSource(source, [], original);
      onApply(source);
    } catch (caught) { setError(caught instanceof Error ? caught.message : "配置无效"); }
  };
  return (
    <form className="asb-form asb-mcp-create" aria-label="MCP 配置向导" noValidate
      onSubmit={(event) => { event.preventDefault(); apply(); }}
      onKeyDown={(event) => {
        if (event.key === "Escape" && !busy) { event.preventDefault(); event.stopPropagation(); onCancel(); }
      }}>
      <div className="asb-mcp-identity">
        <label className="asb-field"><span>服务名称</span>
          <Input code aria-label="服务名称" value={name} autoFocus disabled={busy}
            onChange={(event) => { setName(event.target.value); setError(null); }} />
        </label>
        <div className="asb-field"><span>传输方式</span>
          <Select ariaLabel="传输方式" value={server.type} options={TRANSPORTS} disabled={busy}
            onChange={chooseTransport} />
        </div>
      </div>
      {pendingType && <div className="asb-mcp-transport-confirm" role="alert">
        <p>更改传输方式将清除不适用的字段及不能沿用的已存凭据。</p>
        <Button variant="secondary" disabled={busy} onClick={() => setPendingType(null)}>取消更改</Button>
        <Button variant="primary" disabled={busy} onClick={() => {
          update(changeTransport(server, pendingType)); setPendingType(null);
        }}>更改传输方式</Button>
      </div>}
      <ServerFields server={server} busy={busy || pendingType !== null} original={original} onChange={update} />
      {error && <p className="asb-warn-text asb-mcp-error" role="alert">{error}</p>}
      <div className="asb-mcp-actions">
        <Button variant="secondary" disabled={busy} onClick={onCancel}><CloseIcon />取消</Button>
        <Button type="submit" variant="primary" disabled={busy || pendingType !== null}><CheckIcon />应用配置</Button>
      </div>
    </form>
  );
}
