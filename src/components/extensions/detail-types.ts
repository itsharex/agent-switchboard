import type {
  BindingStatus,
  ClientCapabilityReport,
  ExtensionListItem,
  ProjectRegistration,
  SkillUpdateReport,
} from "../../api/client";

export interface ExtensionDetailProps {
  item: ExtensionListItem;
  projects: ProjectRegistration[];
  capabilities: ClientCapabilityReport[];
  busy: boolean;
  installTargets: string[];
  updateReport: SkillUpdateReport | null;
  projectNames: ReadonlyMap<string, string>;
  onInstallTargetsChange: (values: string[]) => void;
  onInstall: () => void;
  onChangeBinding: (binding: BindingStatus, enable: boolean) => void;
  onRemoveBinding: (binding: BindingStatus) => void;
  onToggleLock: (binding: BindingStatus, locked: boolean) => void;
  onDelete: () => void;
  onEditMcp: () => void;
  onEditSkillContent: () => void;
  onCheckUpdates: () => void;
  onApplyUpdate: () => void;
  onDeployCurrent: () => void;
  onExportPortable?: () => void;
}
