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
import { AppDialog } from "../AppDialog";
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
    </div>
  );
}

/** The only resident confirmation for extension writes: shown when a
 * prepared plan would write sensitive connection data. Every other write
 * applies immediately through the shared pipeline. */
export function ExtensionPlanSheet({ view, busy, projectNames, resourceNames, onConfirm, onCancel }: Props) {
  const operation =
    view.operations.find((entry) => entry.operation === "install" || entry.operation === "remove")
      ?.operation ??
    view.operations[0]?.operation ??
    "update";
  return (
    <AppDialog
      title={`确认${OPERATION_LABELS[operation]}（写入敏感数据）`}
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
            确认写入
          </Button>
        </>
      }
    >
      <p className="asb-scope-note">
        本次变更会向客户端配置写入连接地址、参数或凭据值；预览中的值已脱敏。
        {view.operations.length > 1 && " 本预览包含多个资源，确认后将在同一事务中一起应用或一起回滚。"}
      </p>
      {view.operations.map((entry, index) => (
        <section key={index} className="asb-ext-section">
          <h3 className="asb-section-title">
            {resourceNames.get(entry.definitionId) ?? entry.definitionId}
            <span className="asb-scope-note"> · {OPERATION_LABELS[entry.operation]}</span>
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
    </AppDialog>
  );
}

/** One confirmation for the whole deletion: bindings are removed first (each
 * restorable from the transaction history), then the definition is deleted. */
export function ExtensionRemoveSheet({
  item,
  busy,
  onConfirm,
  onCancel,
}: {
  item: ExtensionListItem;
  busy: boolean;
  onConfirm: () => void;
  onCancel: () => void;
}) {
  const bound = item.bindings.length > 0;
  return (
    <AppDialog
      title="删除扩展定义"
      busy={busy}
      onClose={onCancel}
      footer={
        <>
          <Button variant="secondary" autoFocus disabled={busy} onClick={onCancel}>
            取消
          </Button>
          <Button variant="danger" disabled={busy} onClick={onConfirm}>
            {bound ? "删除并撤销部署" : "确认删除"}
          </Button>
        </>
      }
    >
      <p>将「{item.name}」从扩展库删除。</p>
      {bound ? (
        <p>
          该扩展仍有 {item.bindings.length} 个客户端安装，将先一并撤销；客户端配置会恢复到部署前的内容，也可随时在操作历史中恢复。
        </p>
      ) : (
        <p>此操作不改动任何客户端配置文件。</p>
      )}
    </AppDialog>
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
    <AppDialog
      title="选择 Claude 项目 Skill 停用范围"
      busy={busy}
      onClose={onCancel}
      footer={
        <>
          <Button variant="secondary" autoFocus disabled={busy} onClick={onCancel}>
            取消
          </Button>
          <Button variant="primary" disabled={busy || sharedSettings === null} onClick={onConfirm}>
            执行停用
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
    </AppDialog>
  );
}
