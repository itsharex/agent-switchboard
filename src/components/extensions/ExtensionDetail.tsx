import { Button } from "../Button";
import { Time } from "../Time";
import { CapabilityPanel } from "./CapabilityPanel";
import { ExtensionBindings, ExtensionInstallTargets } from "./ExtensionBindings";
import { ExtensionCheckPanel } from "./ExtensionCheckPanel";
import { ExtensionResourceFacts } from "./ExtensionResourceFacts";
import type { ExtensionDetailProps } from "./detail-types";
import { SKILL_CHANGE_LABELS, parseTargetValue } from "./labels";

function DetailActions(props: ExtensionDetailProps) {
  const { item, busy } = props;
  return (
    <div className="asb-ext-actions">
      <Button
        variant="secondary"
        disabled={busy}
        onClick={item.kind === "skill" ? props.onEditSkillContent : props.onEditMcp}
      >
        {item.kind === "skill" ? "编辑内容" : "编辑定义"}
      </Button>
      {item.kind === "skill" && (
        <Button variant="secondary" disabled={busy} onClick={props.onCheckUpdates}>
          检查更新
        </Button>
      )}
      {props.onExportPortable && (
        <Button variant="secondary" disabled={busy} onClick={props.onExportPortable}>
          导出便携包
        </Button>
      )}
      {item.bindings.some(
        (binding) => binding.desired === "enabled" && binding.fileState === "pendingApply",
      ) && (
        <Button variant="secondary" disabled={busy} onClick={props.onDeployCurrent}>
          预览部署当前版本
        </Button>
      )}
      <Button variant="danger" disabled={busy} onClick={props.onDelete}>
        删除定义
      </Button>
    </div>
  );
}

function SkillUpdateDetails({ item, updateReport: report, busy, onApplyUpdate }: ExtensionDetailProps) {
  if (item.kind !== "skill" || !report) return null;
  if (report.error)
    return (
      <p className="asb-warn-text" role="alert">
        检查更新失败：{report.error}
      </p>
    );
  const current = report.upToDate || report.newDigest === item.contentDigest;
  return (
    <div className="asb-ext-section">
      <div className="asb-ext-actions">
        <span className="asb-scope-note">
          {current
            ? "来源内容已是最新"
            : `发现新版本${report.newCommit ? `（提交 ${report.newCommit.slice(0, 12)}）` : ""}`}
        </span>
        {!current && (
          <Button variant="secondary" disabled={busy || !report.newDigest} onClick={onApplyUpdate}>
            更新到新内容
          </Button>
        )}
      </div>
      {!current && report.changedFiles.length > 0 && (
        <>
          <h4>与来源的文件差异</h4>
          <ul className="asb-ext-file-list">
            {report.changedFiles.map((file) => (
              <li key={`${file.action}-${file.relativePath}`}>
                {SKILL_CHANGE_LABELS[file.action]} {file.relativePath}
              </li>
            ))}
          </ul>
        </>
      )}
    </div>
  );
}

export function ExtensionDetail(props: ExtensionDetailProps) {
  const { item, installTargets, busy, capabilities } = props;
  const checkTarget = installTargets.length > 0 ? parseTargetValue(installTargets[0]) : null;
  return (
    <section className="asb-ext-detail" aria-label={`扩展详情 ${item.name}`}>
      <div className="asb-ext-detail-main">
        <DetailActions {...props} />
        <ExtensionResourceFacts item={item} />
        <SkillUpdateDetails {...props} />
        <ExtensionBindings {...props} />
        <ExtensionInstallTargets {...props} />
        {item.kind === "mcp" && (
          <ExtensionCheckPanel definitionId={item.id} target={checkTarget} busy={busy} />
        )}
        <p className="asb-scope-note">
          更新于 <Time iso={item.updatedAt} /> · 修订 r{item.revision}
        </p>
      </div>
      <CapabilityPanel reports={capabilities} />
    </section>
  );
}
