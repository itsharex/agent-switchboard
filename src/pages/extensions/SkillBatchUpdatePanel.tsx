import type { SkillUpdateReport } from "../../api/client";
import { Button } from "../../components/Button";
import { Checkbox } from "../../components/Checkbox";
import { SKILL_CHANGE_LABELS } from "../../components/extensions/labels";

interface Props {
  reports: SkillUpdateReport[];
  checked: ReadonlySet<string>;
  names: ReadonlyMap<string, string>;
  busy: boolean;
  onCheckedChange: (definitionId: string, checked: boolean) => void;
  onPreview: () => void;
  onClose: () => void;
}

/** One prepared batch of source-freshness reports: per-skill status with
 * the file-level difference, and the subset the next combined deployment
 * preview will cover. Checking a skill includes it in that preview; a
 * failed check never removes the other rows. */
export function SkillBatchUpdatePanel({
  reports,
  checked,
  names,
  busy,
  onCheckedChange,
  onPreview,
  onClose,
}: Props) {
  const previewable = reports.filter(
    (report) => checked.has(report.definitionId) && report.newDigest !== null,
  );
  return (
    <div className="asb-ext-section" aria-label="批量更新检查结果">
      <div className="asb-ext-history-head">
        <h4>批量更新检查</h4>
        <div className="asb-ext-actions">
          <Button
            variant="primary"
            disabled={busy || previewable.length === 0}
            onClick={onPreview}
          >
            预览批量更新（{previewable.length}）
          </Button>
          <Button variant="secondary" disabled={busy} onClick={onClose}>
            关闭
          </Button>
        </div>
      </div>
      <p className="asb-scope-note">
        勾选的 Skill 会进入同一份部署预览，在同一个事务中一起应用或一起回滚。
      </p>
      <ul className="asb-ext-binding-list">
        {reports.map((report) => {
          const name = names.get(report.definitionId) ?? report.definitionId;
          return (
            <li key={report.definitionId} className="asb-ext-binding">
              {report.error !== null ? (
                <span className="asb-warn-text">
                  {name}：检查失败（{report.error}）
                </span>
              ) : report.upToDate ? (
                <span className="asb-scope-note">{name}：来源已是最新</span>
              ) : (
                <Checkbox
                  checked={checked.has(report.definitionId)}
                  label={`${name}：可更新${
                    report.newCommit ? `，新提交 ${report.newCommit.slice(0, 12)}` : ""
                  }`}
                  ariaLabel={`将 ${name} 包含进批量更新`}
                  disabled={busy || report.newDigest === null}
                  onChange={(next) => onCheckedChange(report.definitionId, next)}
                />
              )}
              {report.changedFiles.length > 0 && (
                <ul className="asb-ext-file-list">
                  {report.changedFiles.map((file) => (
                    <li key={`${file.action}-${file.relativePath}`} className="asb-code">
                      {SKILL_CHANGE_LABELS[file.action]} {file.relativePath}
                    </li>
                  ))}
                </ul>
              )}
            </li>
          );
        })}
      </ul>
    </div>
  );
}
