import { useRef, useState } from "react";
import type {
  CodexServerOptions,
  FieldEdit,
  McpDefinition,
  McpEditRequest,
  McpEditViewEnvelope,
  McpFieldEdits,
  SecretValue,
} from "../../api/client";
import { Button } from "../Button";
import { Input } from "../Input";
import { Select, type SelectOption } from "../Select";
import { BearerCredentialField } from "./mcp-edit/BearerCredentialField";
import { CodexOptionsSection } from "./mcp-edit/CodexOptionsSection";
import { SlotRowsEditor } from "./mcp-edit/SlotRowsEditor";
import {
  initialSlotDraft,
  optionsEqual,
  parseArgs,
  slotUnchanged,
  type BearerDraft,
  type SlotDraft,
} from "./mcp-edit/slots";

interface Props {
  envelope: McpEditViewEnvelope;
  busy: boolean;
  onPutSecret: (value: string, purpose: string) => Promise<string | null>;
  onSave: (edit: McpEditRequest) => Promise<boolean>;
  onCancel: () => void;
}

const TRANSPORT_OPTIONS: SelectOption[] = [
  { value: "stdio", label: "stdio（本地命令）" },
  { value: "http", label: "HTTP（远程端点）" },
  { value: "claudeSse", label: "SSE（仅 Claude）" },
  { value: "claudeWs", label: "WebSocket（仅 Claude）" },
];

/** The editor for one existing MCP definition. Fields the user does not
 * touch are omitted from the request, so stored credentials survive an edit
 * without their values ever returning to the renderer. */
export function McpEditForm({ envelope, busy, onPutSecret, onSave, onCancel }: Props) {
  const currentTransport = envelope.transport;
  const [name, setName] = useState(envelope.name);
  const [transport, setTransport] = useState(currentTransport);
  const [command, setCommand] = useState(envelope.transport === "stdio" ? envelope.command : "");
  const [argsText, setArgsText] = useState(
    envelope.transport === "stdio" ? envelope.args.join("\n") : "",
  );
  const [url, setUrl] = useState("url" in envelope ? envelope.url : "");
  const [envRows, setEnvRows] = useState<SlotDraft[]>(
    envelope.transport === "stdio"
      ? envelope.env.map((slot, index) => initialSlotDraft(slot, index + 1))
      : [],
  );
  const [headerRows, setHeaderRows] = useState<SlotDraft[]>(
    envelope.transport === "http" || envelope.transport === "claudeSse" || envelope.transport === "claudeWs"
      ? envelope.headers.map((slot, index) => initialSlotDraft(slot, index + 1))
      : [],
  );
  const [bearer, setBearer] = useState<BearerDraft>(
    envelope.transport === "http"
      ? {
          initial: envelope.bearer,
          kind:
            envelope.bearer === null
              ? "none"
              : envelope.bearer.mode === "secretConfigured"
                ? "keep"
                : envelope.bearer.mode === "envRef"
                  ? "envRef"
                  : "plain",
          text:
            envelope.bearer?.mode === "envRef"
              ? envelope.bearer.name
              : envelope.bearer?.mode === "plain"
                ? envelope.bearer.value
                : "",
        }
      : { initial: null, kind: "none", text: "" },
  );
  const initialOptions = envelope.transport === "stdio" ? envelope.codexOptions : null;
  const [optionsCwd, setOptionsCwd] = useState(initialOptions?.cwd ?? "");
  const [optionsStartup, setOptionsStartup] = useState(
    initialOptions?.startupTimeoutSec != null ? String(initialOptions.startupTimeoutSec) : "",
  );
  const [optionsTool, setOptionsTool] = useState(
    initialOptions?.toolTimeoutSec != null ? String(initialOptions.toolTimeoutSec) : "",
  );
  const [optionsRequired, setOptionsRequired] = useState<string>(
    initialOptions?.required == null ? "" : initialOptions.required ? "true" : "false",
  );
  const [error, setError] = useState<string | null>(null);
  const nextRowId = useRef(1);

  const switchingTransport = transport !== currentTransport;

  const buildSlotValue = async (row: SlotDraft): Promise<SecretValue | null> => {
    if (row.kind === "plain") return { mode: "plain", value: row.text };
    if (row.kind === "envRef") return { mode: "envRef", name: row.text };
    const reference = await onPutSecret(row.text, `mcp:${name}:${row.name.trim()}`);
    if (reference === null) return null;
    return { mode: "secretRef", reference };
  };

  const slotPatches = async (
    rows: SlotDraft[],
    initialNames: Set<string>,
    label: string,
  ): Promise<Record<string, FieldEdit<SecretValue>> | null> => {
    const patches: Record<string, FieldEdit<SecretValue>> = {};
    const seen = new Set<string>();
    for (const row of rows) {
      const key = row.name.trim();
      if (key) {
        if (seen.has(key)) {
          setError(`${label} ${key} 重复`);
          return null;
        }
        seen.add(key);
      }
      // A kept stored credential has no visible value by design; it stays
      // unnamed in the request and the stored value survives.
      if (row.kind === "keep") continue;
      if (!key && !row.text.trim()) continue;
      if (!key || !row.text.trim()) {
        setError(`${label}的键与值需成对填写`);
        return null;
      }
      if (slotUnchanged(row)) continue;
      const value = await buildSlotValue(row);
      if (value === null) {
        setError("凭据保存失败；请重试后再次保存");
        return null;
      }
      patches[key] = { action: "replace", value };
    }
    for (const initial of initialNames) {
      if (!seen.has(initial)) {
        patches[initial] = { action: "delete" };
      }
    }
    return patches;
  };

  const submit = async () => {
    setError(null);
    if (!name.trim()) {
      setError("服务名称不能为空");
      return;
    }
    if (switchingTransport) {
      const definition = await buildTransportDefinition();
      if (definition === null) return;
      const saved = await onSave({
        expectedRevision: envelope.revision,
        transport: definition,
        fields: {},
      });
      if (saved) onCancel();
      return;
    }
    const fields: McpFieldEdits = {};
    if (currentTransport === "stdio") {
      if (command.trim() !== envelope.command) {
        fields.command = { action: "replace", value: command.trim() };
      }
      const nextArgs = parseArgs(argsText);
      if (nextArgs.join("\n") !== envelope.args.join("\n")) {
        fields.args = { action: "replace", value: nextArgs };
      }
      const env = await slotPatches(envRows, new Set(envelope.env.map((slot) => slot.name)), "环境变量");
      if (env === null) return;
      if (Object.keys(env).length > 0) fields.env = env;
      const built = buildOptions();
      if (!built.ok) return;
      if (!optionsEqual(built.options, initialOptions)) {
        fields.codexOptions =
          built.options === null
            ? { action: "delete" }
            : { action: "replace", value: built.options };
      }
    } else {
      if (url.trim() !== envelope.url) {
        fields.url = { action: "replace", value: url.trim() };
      }
      const headers = await slotPatches(
        headerRows,
        new Set(envelope.headers.map((slot) => slot.name)),
        "请求头",
      );
      if (headers === null) return;
      if (Object.keys(headers).length > 0) fields.headers = headers;
      if (currentTransport === "http") {
        const bearerPatch = await buildBearerPatch();
        if (bearerPatch === undefined) return;
        if (bearerPatch !== null) fields.bearer = bearerPatch;
      }
    }
    const saved = await onSave({
      expectedRevision: envelope.revision,
      ...(name.trim() !== envelope.name ? { serverKey: name.trim() } : {}),
      fields,
    });
    if (saved) onCancel();
  };

  /** `ok: false` marks invalid input (the error is already set); a null
   * `options` value inside `ok: true` means every field is empty. */
  const buildOptions = (): { ok: boolean; options: CodexServerOptions | null } => {
    const cwd = optionsCwd.trim();
    const required = optionsRequired;
    if (!cwd && !optionsStartup.trim() && !optionsTool.trim() && !required) {
      return { ok: true, options: null };
    }
    const startup = Number(optionsStartup.trim());
    const tool = Number(optionsTool.trim());
    if (optionsStartup.trim() && (!Number.isInteger(startup) || startup < 0)) {
      setError("启动超时必须是正整数秒");
      return { ok: false, options: null };
    }
    if (optionsTool.trim() && (!Number.isInteger(tool) || tool < 0)) {
      setError("工具超时必须是正整数秒");
      return { ok: false, options: null };
    }
    return {
      ok: true,
      options: {
        ...(cwd ? { cwd } : {}),
        ...(optionsStartup.trim() ? { startupTimeoutSec: startup } : {}),
        ...(optionsTool.trim() ? { toolTimeoutSec: tool } : {}),
        ...(required ? { required: required === "true" } : {}),
      },
    };
  };

  /** `null` means the caller should abort; `undefined` never happens for
   * `null` returns are handled through the error state. */
  const buildBearerPatch = async (): Promise<
    { action: "replace"; value: SecretValue } | { action: "delete" } | null | undefined
  > => {
    const current = bearer.initial;
    if (bearer.kind === "keep") return null;
    if (bearer.kind === "none") {
      if (current === null) return null;
      if (current.mode === "plain" && current.value === bearer.text) return null;
      if (current.mode === "envRef" && current.name === bearer.text) return null;
      return { action: "delete" };
    }
    if (!bearer.text.trim()) {
      setError("Bearer 凭据值不能为空；如需移除请选择“不设置”");
      return undefined;
    }
    if (
      current !== null &&
      ((bearer.kind === "plain" && current.mode === "plain" && current.value === bearer.text) ||
        (bearer.kind === "envRef" && current.mode === "envRef" && current.name === bearer.text))
    ) {
      return null;
    }
    const value = await buildSlotValue({
      id: 0,
      name: "bearer",
      kind: bearer.kind,
      text: bearer.text,
      initial: current,
    });
    if (value === null) {
      setError("凭据保存失败；请重试后再次保存");
      return undefined;
    }
    return { action: "replace", value };
  };

  const buildTransportDefinition = async (): Promise<McpDefinition | null> => {
    if (transport === "stdio") {
      if (!command.trim()) {
        setError("stdio 命令不能为空");
        return null;
      }
      const env = await slotPatches(envRows, new Set(), "环境变量");
      if (env === null) return null;
      const envMap: Record<string, SecretValue> = {};
      for (const [key, patch] of Object.entries(env)) {
        if (patch.action === "replace" && patch.value) envMap[key] = patch.value;
      }
      const options = buildOptions();
      if (!options.ok) return null;
      return {
        transport: "stdio",
        command: command.trim(),
        args: parseArgs(argsText),
        env: envMap,
        ...(options.options ? { codexOptions: options.options } : {}),
      };
    }
    if (!url.trim()) {
      setError("服务地址不能为空");
      return null;
    }
    const headers = await slotPatches(headerRows, new Set(), "请求头");
    if (headers === null) return null;
    const headerMap: Record<string, SecretValue> = {};
    for (const [key, patch] of Object.entries(headers)) {
      if (patch.action === "replace" && patch.value) headerMap[key] = patch.value;
    }
    if (transport === "http") {
      let bearerValue: SecretValue | null = null;
      if (bearer.kind !== "none" && bearer.kind !== "keep") {
        if (!bearer.text.trim()) {
          setError("Bearer 凭据值不能为空；如不需请选择“不设置”");
          return null;
        }
        const built = await buildSlotValue({
          id: 0,
          name: "bearer",
          kind: bearer.kind,
          text: bearer.text,
          initial: null,
        });
        if (built === null) {
          setError("凭据保存失败；请重试后再次保存");
          return null;
        }
        bearerValue = built;
      }
      return {
        transport: "http",
        url: url.trim(),
        headers: headerMap,
        ...(bearerValue ? { bearer: bearerValue } : {}),
      };
    }
    return {
      transport,
      url: url.trim(),
      headers: headerMap,
    };
  };

  const showsStdioFields = switchingTransport ? transport === "stdio" : currentTransport === "stdio";
  const showsRemoteFields = switchingTransport
    ? transport !== "stdio"
    : currentTransport !== "stdio";

  return (
    <form
      className="asb-form"
      aria-label="编辑 MCP 服务"
      onSubmit={(event) => {
        event.preventDefault();
        void submit();
      }}
    >
      <label className="asb-field">
        <span>服务名称（客户端服务键；修改后部署时迁移现有条目到新键）</span>
        <Input
          required
          value={name}
          disabled={busy}
          onChange={(event) => setName(event.target.value)}
        />
      </label>
      <div className="asb-field">
        <span>传输方式{switchingTransport ? "（切换后将完整重定义）" : ""}</span>
        <Select
          value={transport}
          options={TRANSPORT_OPTIONS}
          onChange={(value) => setTransport(value as McpDefinition["transport"])}
          ariaLabel="传输方式"
          disabled={busy}
        />
      </div>
      {switchingTransport && (
        <p className="asb-scope-note" role="note">
          切换传输不会携带旧传输的任何字段；请完整填写新传输配置。
        </p>
      )}
      {showsStdioFields && (
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
            <textarea
              className="asb-input asb-textarea"
              aria-label="启动参数"
              rows={3}
              value={argsText}
              disabled={busy}
              onChange={(event) => setArgsText(event.target.value)}
            />
          </label>
          <SlotRowsEditor
            busy={busy}
            rows={envRows}
            setRows={setEnvRows}
            label="环境变量"
            keyPlaceholder="变量名"
            nextRowId={nextRowId}
          />
          <CodexOptionsSection
            busy={busy}
            optionsCwd={optionsCwd}
            setOptionsCwd={setOptionsCwd}
            optionsStartup={optionsStartup}
            setOptionsStartup={setOptionsStartup}
            optionsTool={optionsTool}
            setOptionsTool={setOptionsTool}
            optionsRequired={optionsRequired}
            setOptionsRequired={setOptionsRequired}
          />
        </>
      )}
      {showsRemoteFields && (
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
          <SlotRowsEditor
            busy={busy}
            rows={headerRows}
            setRows={setHeaderRows}
            label="请求头"
            keyPlaceholder="Header 名"
            nextRowId={nextRowId}
          />
          {(switchingTransport ? transport === "http" : currentTransport === "http") && (
            <BearerCredentialField busy={busy} bearer={bearer} setBearer={setBearer} />
          )}
        </>
      )}
      {error && (
        <p className="asb-warn-text" role="alert">
          {error}
        </p>
      )}
      <p className="asb-scope-note">
        未修改的字段（含已存凭据）保持原值；已部署客户端的更新仍需在部署预览中确认。
      </p>
      <div className="asb-form-actions">
        <Button type="submit" variant="primary" disabled={busy}>
          保存修改
        </Button>
        <Button type="button" variant="secondary" disabled={busy} onClick={onCancel}>
          取消
        </Button>
      </div>
    </form>
  );
}
