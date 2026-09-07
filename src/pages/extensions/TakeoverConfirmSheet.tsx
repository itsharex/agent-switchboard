import type { TakeoverPreview } from "../../api/client";
import { ConfirmSheet } from "../../components/ConfirmSheet";
import { clientName } from "../../lib/client-name";

interface Props {
  takeoverPreview: TakeoverPreview;
  confirmTakeover: () => void;
  cancelTakeover: () => void;
}

/** Confirms one discovered-extension takeover from its redacted preview.
 * The panel states exactly the three things the user must understand:
 * which client and scope will be managed, that current files stay as they
 * are, and what happens to later changes when the management is undone —
 * managing includes adding to the library, so no separate copy is needed. */
export function TakeoverConfirmSheet({ takeoverPreview, confirmTakeover, cancelTakeover }: Props) {
  const scope = `${clientName(takeoverPreview.client)} · ${takeoverPreview.scopeLabel}`;
  const restoreLine = takeoverPreview.warnings.find((warning) => warning.includes("恢复"));
  const keepLine = takeoverPreview.warnings.find((warning) => warning !== restoreLine);
  const restoreFallback = "解除管理时，将恢复管理开始时记录的原始状态";
  return (
    <ConfirmSheet
      title={`管理现有安装：${takeoverPreview.name}`}
      details={[
        <p key="scope">
          <strong>管理对象</strong>：{scope}
        </p>,
        <p key="keep">
          <strong>现有文件保持原样</strong>
          {keepLine ? `：${keepLine}` : null}
          {takeoverPreview.fileCount !== null && takeoverPreview.fileCount !== undefined
            ? `（共 ${takeoverPreview.fileCount} 个文件）`
            : null}
        </p>,
        <p key="restore">
          <strong>解除管理时</strong>：{restoreLine ?? restoreFallback}
        </p>,
        takeoverPreview.nativeEntryPresent === true ? (
          <p key="baseline" className="asb-scope-note">
            将记录当前原生条目作为所有权基线
          </p>
        ) : null,
        takeoverPreview.contentDigest ? (
          <p key="digest" className="asb-scope-note">
            内容摘要 {takeoverPreview.contentDigest}
          </p>
        ) : null,
        takeoverPreview.definitionExists ? (
          <p key="exists" className="asb-scope-note">
            库中已有等价定义，将复用
          </p>
        ) : null,
        <p key="hint" className="asb-scope-note">
          管理现有安装已包含加入扩展库，无需先复制。
        </p>,
      ].filter((detail): detail is NonNullable<typeof detail> => detail !== null)}
      confirmLabel="确认管理"
      onConfirm={() => void confirmTakeover()}
      onCancel={cancelTakeover}
    />
  );
}
