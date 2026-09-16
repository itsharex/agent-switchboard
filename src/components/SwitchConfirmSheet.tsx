import type { FilePreview } from "../api/client";
import { ConfirmSheet } from "./ConfirmSheet";
import { PreviewInspector } from "./PreviewInspector";

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
        <PreviewInspector
          filePreview={filePreview}
          userConfigModel={null}
          userConfigWarnings={[]}
        />,
      ]}
      confirmLabel="确认切换"
      confirmDisabled={busy}
      onConfirm={onConfirm}
      onCancel={onCancel}
    />
  );
}
