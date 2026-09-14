import { useEffect, useState } from "react";
import { executeSwitch, previewSwitch, type FilePreview } from "../../api/switching";
import { Button } from "../Button";
import { PreviewInspector } from "../PreviewInspector";
import type { CodexOperations } from "./operations";
export function SwitchConfirmation({ profileId, operations, onClose }: { profileId: string; operations: CodexOperations; onClose: () => void }) {
  const [preview, setPreview] = useState<FilePreview | null>(null);
  const { run, busy, changed } = operations;
  useEffect(() => { void run(async () => setPreview(await previewSwitch(profileId))); }, [profileId, run]);
  return <section aria-label="Codex 配置应用确认">
    <PreviewInspector filePreview={preview} userConfigModel={null} userConfigWarnings={[]} />
    <div className="asb-form-actions">
      <Button variant="secondary" disabled={busy} onClick={onClose}>取消应用</Button>
      <Button variant="primary" disabled={busy || !preview} onClick={() => void run(async () => {
        if (!preview) return;
        await executeSwitch(profileId, preview.contentHash, preview.renderedHash, true, preview);
        changed("Codex 配置已应用；请重新启动需要读取配置的会话。"); onClose();
      })}>确认应用 Codex 配置</Button>
    </div>
  </section>;
}
