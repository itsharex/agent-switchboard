import { uiMessage } from "../../../i18n/errors";
import { useState } from "react";
import type { AppKind, ExtensionDraft } from "../../../api/client";
import { materializeMcpDraft, validateMcpSource, type PutSecret } from "./mcp-draft";
import { emptyServer, formatMcpJson, jsonServiceName, parseMcpJson, renameJsonService, type CreateSource } from "./mcp-json";
import { validateUniqueName, uniquePresetName } from "./mcp-name";
import { buildMetadata, metadataDraft, type MetadataDraft } from "./metadata";
import { presetMetadata, presetServer } from "./presets";
import { useMcpSubmission } from "./useMcpSubmission";

export interface NewMcpFormProps {
  busy: boolean;
  existingNames?: string[];
  onBusyChange?: (busy: boolean) => void;
  onPutSecret: PutSecret;
  onSave: (draft: ExtensionDraft, clients: AppKind[]) => Promise<boolean>;
}

const DEFAULT_CLIENTS: AppKind[] = ["codex", "claude"];
interface EditorDocument { name: string; json: string }
const EMPTY_DOCUMENT: EditorDocument = { name: "", json: "" };

async function saveNewMcp(props: NewMcpFormProps, document: EditorDocument, clients: AppKind[], metadata: MetadataDraft) {
  const source = parseMcpJson(document.json, document.name);
  validateUniqueName(source.name, props.existingNames ?? []);
  validateMcpSource(source, clients);
  const mcpMetadata = buildMetadata(metadata);
  const draft = await materializeMcpDraft(source, props.onPutSecret);
  if (!await props.onSave({ ...draft, ...(mcpMetadata ? { mcpMetadata } : {}) }, [...clients])) {
    throw uiMessage("mcp.error.saveIncomplete");
  }
}

export function useMcpForm(props: NewMcpFormProps) {
  const [document, setDocument] = useState(EMPTY_DOCUMENT);
  const [clients, setClients] = useState([...DEFAULT_CLIENTS]);
  const [metadata, setMetadata] = useState(metadataDraft);
  const [preset, setPreset] = useState("custom");
  const [wizard, setWizard] = useState<CreateSource | null>(null);
  const [focusJson, setFocusJson] = useState(false);
  const submission = useMcpSubmission(props.busy, async () => {
    await saveNewMcp(props, document, clients, metadata);
    setDocument(EMPTY_DOCUMENT);
    setMetadata(metadataDraft());
    setClients([...DEFAULT_CLIENTS]);
    setPreset("custom");
    setFocusJson(true);
  }, props.onBusyChange);
  const { setError } = submission;
  const changeJson = (json: string) => {
    setDocument((current) => ({ json, name: jsonServiceName(json) ?? current.name }));
    setPreset("custom");
    setError(null);
  };
  const changeName = (name: string) => {
    setDocument((current) => ({ name, json: renameJsonService(current.json, name) }));
    setError(null);
  };
  const changeMetadata = (next: MetadataDraft) => { setMetadata(next); setError(null); };
  const selectPreset = (id: string) => {
    setDocument(id === "custom" ? EMPTY_DOCUMENT : {
      name: uniquePresetName(id, props.existingNames ?? []), json: formatMcpJson(presetServer(id)),
    });
    setMetadata(metadataDraft(id === "custom" ? undefined : presetMetadata(id)));
    setPreset(id);
    setError(null);
  };
  const openWizard = () => {
    try {
      setWizard(document.json.trim() ? parseMcpJson(document.json, document.name)
        : { name: document.name, server: emptyServer("stdio") });
      setError(null);
    } catch (caught) { setError(caught); }
  };
  const closeWizard = () => { setWizard(null); setFocusJson(true); };
  const applyWizard = (source: CreateSource) => {
    setDocument({ name: source.name, json: formatMcpJson(source.server) });
    setPreset("custom");
    setError(null);
    closeWizard();
  };
  const toggleClient = (client: AppKind) => {
    setClients((current) => DEFAULT_CLIENTS.filter((entry) => entry === client ? !current.includes(entry) : current.includes(entry)));
    setError(null);
  };
  return { ...document, ...submission, clients, metadata, preset, wizard, focusJson,
    changeJson, changeName, changeMetadata, selectPreset, openWizard, closeWizard, applyWizard, toggleClient };
}
