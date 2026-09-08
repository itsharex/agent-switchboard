import type { TakeoverPreview } from "../../api/client";
import { Button } from "../../components/Button";
import { ExtensionDialog } from "../../components/extensions/ExtensionDialog";
import { clientName } from "../../lib/client-name";

interface Props {
  takeoverPreview: TakeoverPreview;
  busy: boolean;
  confirmTakeover: () => void;
  cancelTakeover: () => void;
}

export function TakeoverConfirmSheet({
  takeoverPreview: preview,
  busy,
  confirmTakeover,
  cancelTakeover,
}: Props) {
  return (
    <ExtensionDialog
      title={`管理现有安装：${preview.name}`}
      busy={busy}
      onClose={cancelTakeover}
      footer={
        <>
          <Button variant="secondary" autoFocus disabled={busy} onClick={cancelTakeover}>
            取消
          </Button>
          <Button variant="primary" disabled={busy} onClick={confirmTakeover}>
            确认管理
          </Button>
        </>
      }
    >
      <p>
        <strong>管理对象</strong>：{clientName(preview.client)} · {preview.scopeLabel}
      </p>
      <p>
        <strong>现有文件保持原样</strong>
        {preview.fileCount != null ? `（共 ${preview.fileCount} 个文件）` : ""}
      </p>
      <p>
        <strong>解除管理时</strong>：恢复管理开始时记录的原始状态。
      </p>
      {preview.warnings.map((warning) => (
        <p className="asb-scope-note" key={warning}>
          {warning}
        </p>
      ))}
      {preview.definitionExists && <p className="asb-scope-note">库中已有等价定义，将复用。</p>}
      <p className="asb-scope-note">管理现有安装已包含加入扩展库，无需先复制。</p>
    </ExtensionDialog>
  );
}
