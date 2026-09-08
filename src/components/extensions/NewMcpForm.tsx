import { useState } from "react";
import type { ExtensionDraft, ExtensionPayload, SecretValue } from "../../api/client";
import { Button } from "../Button";
import { Checkbox } from "../Checkbox";
import { Input } from "../Input";
import { Textarea } from "../Textarea";
import { Select } from "../Select";

interface SecretRow {
  key: string;
  value: string;
  useSecret: boolean;
}

interface Props {
  busy: boolean;
  onPutSecret: (value: string, purpose: string) => Promise<string | null>;
  onSave: (draft: ExtensionDraft) => Promise<boolean>;
}

const EMPTY_ROW: SecretRow = { key: "", value: "", useSecret: false };

const TRANSPORT_OPTIONS = [
  { value: "stdio", label: "stdio（本地命令）" },
  { value: "http", label: "HTTP（远程端点）" },
] as const;

function parseArgs(text: string): string[] {
  return text
    .split("\n")
    .map((line) => line.trim())
    .filter((line) => line.length > 0);
}

type RowsResult =
  | { kind: "ok"; map: Record<string, SecretValue> }
  | { kind: "unpaired" }
  | { kind: "failed" };

async function rowsToMap(
  rows: SecretRow[],
  build: (row: SecretRow) => Promise<SecretValue | null>,
): Promise<RowsResult> {
  const map: Record<string, SecretValue> = {};
  for (const row of rows) {
    if (!row.key.trim() && !row.value.trim()) continue;
    if (!row.key.trim() || !row.value.trim()) return { kind: "unpaired" };
    const built = await build(row);
    if (built === null) return { kind: "failed" };
    map[row.key.trim()] = built;
  }
  return { kind: "ok", map };
}

/** Manual MCP definition authoring. Sensitive-shaped values are stored as
 * system-credential references before they ever reach the library. */
export function NewMcpForm({ busy, onPutSecret, onSave }: Props) {
  const [name, setName] = useState("");
  const [transport, setTransport] = useState<"stdio" | "http">("stdio");
  const [command, setCommand] = useState("");
  const [argsText, setArgsText] = useState("");
  const [url, setUrl] = useState("");
  const [envRows, setEnvRows] = useState<SecretRow[]>([{ ...EMPTY_ROW }]);
  const [headerRows, setHeaderRows] = useState<SecretRow[]>([{ ...EMPTY_ROW }]);
  const [bearerValue, setBearerValue] = useState("");
  const [bearerUseSecret, setBearerUseSecret] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const buildValue = async (row: SecretRow): Promise<SecretValue | null> => {
    if (row.useSecret) {
      const reference = await onPutSecret(row.value, `mcp:${name}:${row.key.trim()}`);
      if (reference === null) {
        setError("凭据保存失败；请重试后再次保存");
        return null;
      }
      return { mode: "secretRef", reference };
    }
    return { mode: "plain", value: row.value };
  };

  const submit = async () => {
    setError(null);
    let payload: ExtensionPayload;
    if (transport === "stdio") {
      if (!command.trim()) {
        setError("stdio 命令不能为空");
        return;
      }
      const env = await rowsToMap(envRows, buildValue);
      if (env.kind !== "ok") {
        setError(env.kind === "unpaired" ? "环境变量键值需成对填写" : "凭据保存失败；请重试后再次保存");
        return;
      }
      payload = { kind: "mcp", transport: "stdio", command: command.trim(), args: parseArgs(argsText), env: env.map };
    } else {
      if (!url.trim()) {
        setError("服务地址不能为空");
        return;
      }
      const headers = await rowsToMap(headerRows, buildValue);
      if (headers.kind !== "ok") {
        setError(headers.kind === "unpaired" ? "请求头键值需成对填写" : "凭据保存失败；请重试后再次保存");
        return;
      }
      let bearer: SecretValue | null = null;
      if (bearerValue.trim()) {
        bearer = await buildValue({ key: "bearer", value: bearerValue, useSecret: bearerUseSecret });
        if (bearer === null) return;
      }
      payload = { kind: "mcp", transport: "http", url: url.trim(), headers: headers.map, ...(bearer ? { bearer } : {}) };
    }
    const saved = await onSave({ name: name.trim(), payload });
    if (saved) {
      setCommand("");
      setArgsText("");
      setUrl("");
      setEnvRows([{ ...EMPTY_ROW }]);
      setHeaderRows([{ ...EMPTY_ROW }]);
      setBearerValue("");
      setBearerUseSecret(false);
    }
  };

  const rowEditor = (
    rows: SecretRow[],
    setRows: (next: SecretRow[]) => void,
    label: string,
    keyPlaceholder: string,
  ) => (
    <div className="asb-ext-section">
      {rows.map((row, index) => (
        <div key={index} className="asb-ext-env-row">
          <Input
            aria-label={`${label}名 ${index + 1}`}
            placeholder={keyPlaceholder}
            value={row.key}
            disabled={busy}
            onChange={(event) =>
              setRows(rows.map((current, i) => (i === index ? { ...current, key: event.target.value } : current)))
            }
          />
          <Input
            aria-label={`${label}值 ${index + 1}`}
            placeholder="值"
            value={row.value}
            disabled={busy}
            onChange={(event) =>
              setRows(rows.map((current, i) => (i === index ? { ...current, value: event.target.value } : current)))
            }
          />
          <Checkbox
            checked={row.useSecret}
            label="存入系统凭据"
            ariaLabel={`${label} ${index + 1} 存入系统凭据`}
            disabled={busy}
            onChange={(checked) =>
              setRows(rows.map((current, i) => (i === index ? { ...current, useSecret: checked } : current)))
            }
          />
          <Button
            variant="secondary"
            disabled={busy}
            aria-label={`移除${label} ${index + 1}`}
            onClick={() => setRows(rows.filter((_, i) => i !== index))}
          >
            删除
          </Button>
        </div>
      ))}
      <Button variant="secondary" disabled={busy} onClick={() => setRows([...rows, { ...EMPTY_ROW }])}>
        添加{label}
      </Button>
    </div>
  );

  return (
    <form
      className="asb-form"
      aria-label="新建 MCP 服务"
      onSubmit={(event) => {
        event.preventDefault();
        void submit();
      }}
    >
      <label className="asb-field">
        <span>服务名称（同时作为客户端服务键）</span>
        <Input
          required
          placeholder="docs-search"
          title="只能包含字母、数字、下划线和连字符，最长 64 个字符"
          value={name}
          disabled={busy}
          onChange={(event) => setName(event.target.value)}
        />
      </label>
      <div className="asb-field">
        <span>传输方式</span>
        <Select
          value={transport}
          options={TRANSPORT_OPTIONS}
          onChange={(value) => setTransport(value as "stdio" | "http")}
          ariaLabel="传输方式"
          disabled={busy}
        />
      </div>
      {transport === "stdio" ? (
        <>
          <label className="asb-field">
            <span>启动命令</span>
            <Input
              required
              placeholder="npx"
              value={command}
              disabled={busy}
              onChange={(event) => setCommand(event.target.value)}
            />
          </label>
          <label className="asb-field">
            <span>启动参数（每行一个）</span>
            <Textarea
              aria-label="启动参数"
              rows={3}
              value={argsText}
              disabled={busy}
              onChange={(event) => setArgsText(event.target.value)}
            />
          </label>
          {rowEditor(envRows, setEnvRows, "环境变量", "变量名")}
        </>
      ) : (
        <>
          <label className="asb-field">
            <span>服务地址</span>
            <Input
              required
              type="url"
              placeholder="https://mcp.example.com/v1"
              value={url}
              disabled={busy}
              onChange={(event) => setUrl(event.target.value)}
            />
          </label>
          {rowEditor(headerRows, setHeaderRows, "请求头", "Header 名")}
          <div className="asb-ext-section">
            <label className="asb-field">
              <span>Bearer 凭据（可选）</span>
              <Input
                type="password"
                autoComplete="off"
                value={bearerValue}
                disabled={busy}
                onChange={(event) => setBearerValue(event.target.value)}
              />
            </label>
            <Checkbox
              checked={bearerUseSecret}
              label="存入系统凭据"
              ariaLabel="Bearer 凭据存入系统凭据"
              disabled={busy}
              onChange={setBearerUseSecret}
            />
          </div>
        </>
      )}
      {error && (
        <p className="asb-warn-text" role="alert">
          {error}
        </p>
      )}
      <div className="asb-form-actions">
        <Button type="submit" variant="primary" disabled={busy}>
          保存到扩展库
        </Button>
      </div>
    </form>
  );
}
