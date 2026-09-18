import type { FilePreview } from "../api/client";
import { Button } from "./Button";
import { PreviewInspector } from "./PreviewInspector";

interface Props {
  filePreview: FilePreview;
  busy: boolean;
  userConfigModel: string | null;
  userConfigWarnings: string[];
  onConfirm: () => void;
  onCancel: () => void;
}

/** The one switch confirmation: unfolds inside the requesting row's card,
 * shows the typed write preview, and never writes on its own. Both client
 * lists mount it identically. */
export function SwitchConfirmationSection({
  filePreview,
  busy,
  userConfigModel,
  userConfigWarnings,
  onConfirm,
  onCancel,
}: Props) {
  return (
    <section className="asb-preview-inline" aria-label="确认切换">
      <div className="asb-panel-heading">
        <h3 className="asb-section-title">确认切换</h3>
        <div className="asb-panel-actions">
          <Button variant="secondary" disabled={busy} onClick={onCancel}>
            取消切换
          </Button>
          <Button variant="primary" disabled={busy} onClick={onConfirm}>
            确认切换
          </Button>
        </div>
      </div>
      <PreviewInspector filePreview={filePreview}
        userConfigModel={userConfigModel} userConfigWarnings={userConfigWarnings} />
    </section>
  );
}
