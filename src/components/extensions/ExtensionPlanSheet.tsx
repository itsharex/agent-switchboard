import type {
  ExtensionListItem,
  ExtensionPlanView,
  KeyChange,
  PlanChangeView,
  PlannedTargetView,
} from "../../api/client";
import { Button } from "../Button";
import { DiffView } from "../DiffView";
import { Select } from "../Select";
import { ExtensionDialog } from "./ExtensionDialog";
import { OPERATION_LABELS, targetLabel } from "./labels";

interface Props {
  view: ExtensionPlanView;
  busy: boolean;
  projectNames: ReadonlyMap<string, string>;
  resourceNames: ReadonlyMap<string, string>;
  onConfirm: () => void;
  onCancel: () => void;
}

function toKeyChange(change: PlanChangeView): KeyChange {
  return {
    key: change.pointer,
    kind: change.after == null ? "remove" : "set",
    before: change.before ?? null,
    after: change.after ?? null,
  };
}

function TargetPreview({ target, label }: { target: PlannedTargetView; label: string }) {
  return (
    <div className="asb-ext-plan-target">
      <h4 className="asb-section-title">{label}</h4>
      {target.warnings.map((warning) => (
        <p key={warning} className="asb-warn-text">
          警告：{warning}
        </p>
      ))}
      {target.changes.length > 0 && (
        <DiffView changes={target.changes.map(toKeyChange)} label={`${label}变更预览`} />
      )}
      {target.files && target.files.length > 0 && (
        <div>
          <p className="asb-scope-note">
            {target.files.some((file) => file.action === "remove") ? "将移除以下文件：" : "将部署以下文件："}
          </p>
          <ul className="asb-ext-file-list">
            {target.files.map((file) => (
              <li key={file.relativePath} className="asb-code">
                {file.relativePath}
              </li>
            ))}
          </ul>
        </div>
      )}
      {target.writesSensitiveConnectionData && (
        <p className="asb-warn-text">该目标会写入已脱敏的连接地址、参数或凭据值</p>
      )}
    </div>
  );
}

export function ExtensionPlanSheet({ view, busy, projectNames, resourceNames, onConfirm, onCancel }: Props) {
  const operation =
    view.operations.find((entry) => entry.operation === "install" || entry.operation === "remove")
      ?.operation ??
    view.operations[0]?.operation ??
    "update";
  return (
    <ExtensionDialog
      title={`${OPERATION_LABELS[operation]}预览`}
      busy={busy}
      onClose={onCancel}
      wide
      footer={
        <>
          <Button variant="secondary" autoFocus disabled={busy} onClick={onCancel}>
            取消
          </Button>
          <Button
            variant={operation === "remove" || operation === "restore" ? "danger" : "primary"}
            disabled={busy}
            onClick={onConfirm}
          >
            确认应用
          </Button>
        </>
      }
    >
      <p className="asb-scope-note">
        应用前会再次校验目标文件未被外部改动；预览中的凭据值已脱敏。
        {view.operations.length > 1 && " 本预览包含多个资源，确认后将在同一事务中一起应用或一起回滚。"}
      </p>
      {view.operations.map((entry, index) => (
        <section key={index} className="asb-ext-section">
          <h3 className="asb-section-title">
            {resourceNames.get(entry.definitionId) ?? entry.definitionId}
            <span className="asb-ext-row-meta"> · {OPERATION_LABELS[entry.operation]}</span>
          </h3>
          {entry.targets.map((target, targetIndex) => (
            <TargetPreview
              key={targetIndex}
              target={target}
              label={targetLabel(target.target, projectNames)}
            />
          ))}
        </section>
      ))}
    </ExtensionDialog>
  );
}

export function ExtensionRemoveSheet({
  item,
  busy,
  onConfirm,
  onCancel,
  onManage,
}: {
  item: ExtensionListItem;
  busy: boolean;
  onConfirm: () => void;
  onCancel: () => void;
  onManage: () => void;
}) {
  const bound = item.bindings.length > 0;
  return (
    <ExtensionDialog
      title="删除扩展定义"
      busy={busy}
      onClose={onCancel}
      footer={
        <>
          <Button variant="secondary" autoFocus disabled={busy} onClick={onCancel}>
            取消
          </Button>
          {bound ? (
            <Button variant="primary" disabled={busy} onClick={onManage}>
              管理安装
            </Button>
          ) : (
            <Button variant="danger" disabled={busy} onClick={onConfirm}>
              确认删除
            </Button>
          )}
        </>
      }
    >
      <p>将「{item.name}」从扩展库删除。</p>
      {bound ? (
        <p>该扩展仍有 {item.bindings.length} 个客户端安装。请先在管理详情中移除这些安装，再删除扩展。</p>
      ) : (
        <p>此操作不改动任何客户端配置文件。</p>
      )}
    </ExtensionDialog>
  );
}

export function SkillDisableScopeSheet({
  sharedSettings,
  busy,
  onSharedSettingsChange,
  onConfirm,
  onCancel,
}: {
  sharedSettings: boolean | null;
  busy: boolean;
  onSharedSettingsChange: (shared: boolean) => void;
  onConfirm: () => void;
  onCancel: () => void;
}) {
  return (
    <ExtensionDialog
      title="选择 Claude 项目 Skill 停用范围"
      busy={busy}
      onClose={onCancel}
      footer={
        <>
          <Button variant="secondary" autoFocus disabled={busy} onClick={onCancel}>
            取消
          </Button>
          <Button variant="primary" disabled={busy || sharedSettings === null} onClick={onConfirm}>
            生成停用预览
          </Button>
        </>
      }
    >
      <p>Claude 的项目 Skill 可见性规则可写入项目共享设置或个人本地设置。请选择本次规则的作用范围。</p>
      <label className="asb-field">
        <span>停用规则写入位置</span>
        <Select
          value={sharedSettings === null ? null : sharedSettings ? "shared" : "local"}
          options={[
            { value: "local", label: "个人本地设置" },
            { value: "shared", label: "项目共享设置" },
          ]}
          onChange={(value) => onSharedSettingsChange(value === "shared")}
          placeholder="选择写入位置"
          ariaLabel="Claude 项目 Skill 停用规则写入位置"
          disabled={busy}
        />
      </label>
    </ExtensionDialog>
  );
}
