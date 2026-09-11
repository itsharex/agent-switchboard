import type { FilePreview } from "../api/client";
import { CodePreview } from "./CodePreview";
import { DiffView } from "./DiffView";
import { PreviewIcon } from "./icons";

interface Props {
  filePreview: FilePreview | null;
  /** Model read from the client's user-level configuration file. */
  userConfigModel: string | null;
  /** Known conditions that can override the user-level configuration. */
  userConfigWarnings: string[];
}

/**
 * The diff inspector. Configuration text stays high-contrast on a near-solid
 * backdrop: no backdrop blur touches this panel (DESIGN.md §6). Preserved
 * host keys are not listed flat — they stay visible in place inside the
 * pretty-printed candidate file.
 */
export function PreviewInspector({
  filePreview,
  userConfigModel,
  userConfigWarnings,
}: Props) {
  if (!filePreview) {
    return (
      <div className="asb-empty-state">
        <span className="asb-empty-state-icon" aria-hidden="true">
          <PreviewIcon />
        </span>
        <h3 className="asb-section-title">选择供应商后生成预览</h3>
      </div>
    );
  }
  const { preview } = filePreview;
  return (
    <div className="asb-inspector">
      <div className="asb-kv">
        <span className="asb-kv-label">当前用户级配置模型</span>
        <span className="asb-kv-value asb-code">{userConfigModel ?? "默认模型"}</span>
      </div>
      {userConfigWarnings.length > 0 && (
        <ul className="asb-warnings" aria-label="范围警告">
          {userConfigWarnings.map((warning) => (
            <li key={warning}>{warning}</li>
          ))}
        </ul>
      )}
      {/* Warnings precede the diff: the gateway rewrite is explained before
          the 127.0.0.1 endpoint change is seen. */}
      {preview.warnings.length > 0 && (
        <ul className="asb-warnings" aria-label="警告">
          {preview.warnings.map((warning) => (
            <li key={warning}>{warning}</li>
          ))}
        </ul>
      )}
      {preview.changes.length === 0 && <p className="asb-empty">无变更</p>}
      <DiffView changes={preview.changes} label="变更键" />
      <CodePreview target={preview.target} content={filePreview.content} />
      <div className="asb-kv">
        <span className="asb-kv-label">备份位置</span>
        <span className="asb-kv-value asb-code">{preview.backupDir}</span>
      </div>
    </div>
  );
}
