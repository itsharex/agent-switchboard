import type { FilePreview } from "../api/client";
import { ConfirmSheet } from "./ConfirmSheet";

interface Props {
  filePreview: FilePreview;
  busy: boolean;
  onConfirm: () => void;
  onCancel: () => void;
}

export function SwitchConfirmSheet({ filePreview, busy, onConfirm, onCancel }: Props) {
  const { preview } = filePreview;
  return (
    <ConfirmSheet
      title="确认切换"
      details={[
        `将写入 ${preview.target}`,
        `变更 ${preview.changes.length} 个键`,
        ...preview.warnings.map((warning) => (
          <span key={warning} className="asb-warn-text">{warning}</span>
        )),
        `备份位置 ${preview.backupDir}`,
      ]}
      confirmLabel="确认切换"
      confirmDisabled={busy}
      onConfirm={onConfirm}
      onCancel={onCancel}
    />
  );
}
