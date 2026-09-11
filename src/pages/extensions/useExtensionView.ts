import { useState } from "react";
import type { ExtensionKind, ExtensionListItem, McpEditViewEnvelope } from "../../api/client";
import type { ExtensionSection } from "../../app/navigation";
import type { ClientFilterValue } from "../../components/ClientFilter";

export type ExtensionDialogView =
  | { type: "detail" | "skillEditor"; definitionId: string }
  | { type: "mcpEditor"; envelope: McpEditViewEnvelope }
  | { type: "remove" | "export"; item: ExtensionListItem }
  | { type: "newMcp" | "newSkill" | "import" | "portableImport" | "project" | "history" };

export interface ExtensionNavigation {
  section: ExtensionSection;
  onSectionChange: (section: ExtensionSection) => void;
}

export function useExtensionView({ section, onSectionChange }: ExtensionNavigation) {
  const [client, setClient] = useState<ClientFilterValue>("all");
  const [search, setSearch] = useState("");
  const [sourceBrowser, setSourceBrowser] = useState(false);
  const [dialog, setDialog] = useState<ExtensionDialogView | null>(null);
  const showDefinition = (id: string, nextKind: ExtensionKind, edit = false) => {
    onSectionChange(nextKind);
    setDialog({ type: edit ? "skillEditor" : "detail", definitionId: id });
  };
  const changeSection = (nextSection: ExtensionSection) => {
    onSectionChange(nextSection);
    setSourceBrowser(false);
    if (nextSection === "instructions") setDialog(null);
  };
  return {
    section,
    kind: section === "instructions" ? null : section,
    changeSection,
    client,
    setClient,
    search,
    setSearch,
    sourceBrowser,
    setSourceBrowser,
    dialog,
    setDialog,
    closeDialog: () => setDialog(null),
    showDefinition,
    clearFilters: () => {
      setClient("all");
      setSearch("");
    },
  };
}
