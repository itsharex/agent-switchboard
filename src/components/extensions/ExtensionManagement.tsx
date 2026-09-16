import type { ReactNode } from "react";
import { Button } from "../Button";
import { PreviewIcon, TrashIcon, UpdateIcon } from "../icons";
import { Time } from "../Time";
import { Tooltip } from "../Tooltip";
import { CapabilityPanel } from "./CapabilityPanel";
import { ExtensionBindings, ExtensionInstallTargets } from "./ExtensionBindings";
import { ExtensionCheckPanel } from "./ExtensionCheckPanel";
import { ExtensionResourceFacts } from "./ExtensionResourceFacts";
import type { ExtensionManagementProps } from "./management-types";
import { SKILL_CHANGE_LABELS, parseTargetValue } from "./labels";

function ManagementAction({ label, disabled, danger = false, onClick, children }: {
  label: string;
  disabled: boolean;
  danger?: boolean;
  onClick: () => void;
  children: ReactNode;
}) {
  return (
    <Tooltip label={label}>
      <Button
        variant="icon"
        className={`asb-ext-management-action${danger ? " is-danger" : ""}`}
        disabled={disabled}
        aria-label={label}
        onClick={onClick}
      >
        {children}
      </Button>
    </Tooltip>
  );
}

function ManagementActions(props: ExtensionManagementProps) {
  const { item, busy } = props;
  return (
    <div className="asb-ext-management-actions" role="group" aria-label={`${item.name} 操作`}>
      {item.kind === "skill" && (
        <ManagementAction label="检查更新" disabled={busy} onClick={props.onCheckUpdates}>
          <UpdateIcon />
        </ManagementAction>
      )}
      {props.onExportPortable && (
        <ManagementAction label="导出便携包" disabled={busy} onClick={props.onExportPortable}>
          <UpdateIcon />
        </ManagementAction>
      )}
      {item.bindings.some(
        (binding) => binding.desired === "enabled" && binding.fileState === "pendingApply",
      ) && (
        <ManagementAction label="部署当前版本" disabled={busy} onClick={props.onDeployCurrent}>
          <PreviewIcon />
        </ManagementAction>
      )}
      <ManagementAction label="删除定义" disabled={busy} danger onClick={props.onDelete}>
        <TrashIcon />
      </ManagementAction>
    </div>
  );
}

function SkillUpdateManagement({ item, updateReport: report, busy, onApplyUpdate }: ExtensionManagementProps) {
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
      <div className="asb-ext-section-heading">
        <h4 className="asb-section-title">来源更新</h4>
        <span className="asb-scope-note">
          {current
            ? "来源内容已是最新"
            : `发现新版本${report.newCommit ? `（提交 ${report.newCommit.slice(0, 12)}）` : ""}`}
        </span>
        {!current && (
          <Button variant="secondary" disabled={busy || !report.newDigest} onClick={onApplyUpdate}>
            更新内容
          </Button>
        )}
      </div>
      {!current && report.changedFiles.length > 0 && (
        <ul className="asb-ext-file-list">
          {report.changedFiles.map((file) => (
            <li key={`${file.action}-${file.relativePath}`}>
              {SKILL_CHANGE_LABELS[file.action]} {file.relativePath}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

export function ExtensionManagement(props: ExtensionManagementProps) {
  const { item, installTargets, busy, capabilities } = props;
  const checkTarget = installTargets.length > 0 ? parseTargetValue(installTargets[0]) : null;
  return (
    <section className="asb-ext-management" aria-label={`部署与诊断 ${item.name}`}>
      <header className="asb-ext-management-head">
        <p className="asb-scope-note">
          {item.kind === "mcp" ? "MCP 服务" : "Skill"} · 更新于 <Time iso={item.updatedAt} /> · 修订 r{item.revision}
        </p>
        <ManagementActions {...props} />
      </header>
      <div className="asb-ext-management-main">
        <ExtensionResourceFacts item={item} />
        <SkillUpdateManagement {...props} />
        {item.kind === "mcp" && (
          <ExtensionCheckPanel definitionId={item.id} target={checkTarget} busy={busy} />
        )}
        <ExtensionBindings {...props} />
        <ExtensionInstallTargets {...props} />
      </div>
      <CapabilityPanel reports={capabilities} />
    </section>
  );
}
