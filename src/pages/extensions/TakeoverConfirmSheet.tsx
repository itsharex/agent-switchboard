import type { TakeoverPreview } from "../../api/client";
import { ConfirmSheet } from "../../components/ConfirmSheet";
import { clientName } from "../../lib/client-name";

interface Props {
  takeoverPreview: TakeoverPreview;
  confirmTakeover: () => void;
  cancelTakeover: () => void;
}

/** Confirms one discovered-extension takeover from its redacted preview. */
export function TakeoverConfirmSheet({ takeoverPreview, confirmTakeover, cancelTakeover }: Props) {
  return (
    <ConfirmSheet
      title={`接管 ${takeoverPreview.name}`}
      details={[
        `${clientName(takeoverPreview.client)} · ${takeoverPreview.scopeLabel}`,
        takeoverPreview.nativeEntryPresent === true
          ? "将记录当前原生条目作为所有权基线"
          : null,
        takeoverPreview.fileCount !== null && takeoverPreview.fileCount !== undefined
          ? `共 ${takeoverPreview.fileCount} 个文件`
          : null,
        takeoverPreview.contentDigest
          ? `内容摘要 ${takeoverPreview.contentDigest}`
          : null,
        takeoverPreview.definitionExists ? "库中已有等价定义，将复用" : null,
        ...takeoverPreview.warnings,
      ].filter((detail): detail is string => detail !== null)}
      confirmLabel="确认接管"
      onConfirm={() => void confirmTakeover()}
      onCancel={cancelTakeover}
    />
  );
}
