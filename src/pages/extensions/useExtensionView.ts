import { useState } from "react";
import type { ExtensionKind, ExtensionListItem, McpEditViewEnvelope } from "../../api/client";
import type { ExtensionSection } from "../../app/navigation";

export type ExtensionDialogView =
  | { type: "detail" | "skillEditor"; definitionId: string }
  | { type: "mcpEditor"; envelope: McpEditViewEnvelope }
  | { type: "remove" | "export"; item: ExtensionListItem }
  | { type: "newMcp" | "newSkill" | "portableImport" | "project" | "history" };

export interface ExtensionNavigation {
  section: ExtensionSection;
  onSectionChange: (section: ExtensionSection) => void;
}

export function useExtensionView({ section, onSectionChange }: ExtensionNavigation) {
  const [search, setSearch] = useState("");
  const [sourceBrowser, setSourceBrowser] = useState(false);
  const [discoveryOpen, setDiscoveryOpen] = useState(false);
  const [dialog, setDialog] = useState<ExtensionDialogView | null>(null);
  const showDefinition = (id: string, nextKind: ExtensionKind, edit = false) => {
    onSectionChange(nextKind);
    setSourceBrowser(false);
    setDiscoveryOpen(false);
    setDialog({ type: edit ? "skillEditor" : "detail", definitionId: id });
  };
  const changeSection = (nextSection: ExtensionSection) => {
    onSectionChange(nextSection);
    if (nextSection === "instructions") {
      setSourceBrowser(false);
      setDiscoveryOpen(false);
      setDialog(null);
    } else if (nextSection !== "skill") {
      setSourceBrowser(false);
    }
  };
  return {
    section,
    kind: section === "instructions" ? null : section,
    changeSection,
    search,
    setSearch,
    sourceBrowser,
    setSourceBrowser,
    discoveryOpen,
    setDiscoveryOpen,
    dialog,
    setDialog,
    closeDialog: () => setDialog(null),
    showDefinition,
    clearFilters: () => setSearch(""),
  };
}
