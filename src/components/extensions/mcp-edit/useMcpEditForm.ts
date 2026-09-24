import { uiMessage } from "../../../i18n/errors";
import { useState } from "react";
import type { AppKind, McpEditRequest, McpEditViewEnvelope } from "../../../api/client";
import type { PutSecret } from "../mcp-create/mcp-draft";
import { formatMcpJson, jsonServiceName, parseMcpJson, renameJsonService, type CreateSource } from "../mcp-create/mcp-json";
import { metadataDraft, type MetadataDraft } from "../mcp-create/metadata";
import { useMcpSubmission } from "../mcp-create/useMcpSubmission";
import { buildMcpEdit } from "./edit-request";
import { mcpEditSource } from "./edit-source";

export interface McpEditFormProps {
  envelope: McpEditViewEnvelope;
  busy: boolean;
  initialClients?: AppKind[];
  existingNames?: string[];
  onBusyChange?: (busy: boolean) => void;
  onPutSecret: PutSecret;
  onSave: (edit: McpEditRequest, clients: AppKind[]) => Promise<boolean>;
  onCancel: () => void;
}

export function useMcpEditForm(props: McpEditFormProps) {
  const original = mcpEditSource(props.envelope);
  const [document, setDocument] = useState(() => ({ name: original.name, json: formatMcpJson(original.server) }));
  const [metadata, setMetadata] = useState(() => metadataDraft(props.envelope.mcpMetadata));
  const [clients, setClients] = useState(() => [...(props.initialClients ?? [])]);
  const [wizard, setWizard] = useState<CreateSource | null>(null);
  const [focusJson, setFocusJson] = useState(false);
  const submission = useMcpSubmission(props.busy, async () => {
    const source = parseMcpJson(document.json, document.name, original.server);
    const request = await buildMcpEdit(props.envelope, source, metadata, clients, props.onPutSecret, props.existingNames);
    if (await props.onSave(request, [...clients])) props.onCancel();
    else throw uiMessage("mcp.error.saveIncomplete");
  }, props.onBusyChange);
  const { setError } = submission;
  const changeName = (name: string) => {
    setDocument((current) => ({ name, json: renameJsonService(current.json, name) }));
    setError(null);
  };
  const changeJson = (json: string) => {
    setDocument((current) => ({ json, name: jsonServiceName(json) ?? current.name }));
    setError(null);
  };
  const changeMetadata = (value: MetadataDraft) => { setMetadata(value); setError(null); };
  const toggleClient = (client: AppKind) => {
    setClients((current) => (["codex", "claude"] as AppKind[])
      .filter((item) => item === client ? !current.includes(item) : current.includes(item)));
    setError(null);
  };
  const openWizard = () => {
    try { setWizard(parseMcpJson(document.json, document.name, original.server)); setError(null); }
    catch (caught) { setError(caught); }
  };
  const closeWizard = () => { setWizard(null); setFocusJson(true); };
  const applyWizard = (source: CreateSource) => {
    setDocument({ name: source.name, json: formatMcpJson(source.server) });
    setError(null);
    closeWizard();
  };
  return { ...document, ...submission, original, metadata, clients, wizard, focusJson,
    changeName, changeJson, changeMetadata, toggleClient, openWizard, closeWizard, applyWizard };
}
