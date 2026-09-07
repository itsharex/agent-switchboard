import type { ReactNode } from "react";
import type { ExtensionPlanView, KeyChange, PlanChangeView } from "../../api/client";
import { ConfirmSheet } from "../ConfirmSheet";
import { DiffView } from "../DiffView";
import { Select } from "../Select";
import { OPERATION_LABELS, targetLabel } from "./labels";

interface Props {
  view: ExtensionPlanView;
  busy: boolean;
  projectNames: ReadonlyMap<string, string>;
  onConfirm: () => void;
  onCancel: () => void;
}

function toKeyChange(change: PlanChangeView): KeyChange {
  return {
    key: change.pointer,
    kind: change.after === undefined || change.after === null ? "remove" : "set",
    before: change.before ?? null,
    after: change.after ?? null,
  };
}

/** The single preview surface for extension plans: every apply goes through
 * this sheet, grouped by resource operation with one block per target. */
export function ExtensionPlanSheet({ view, busy, projectNames, onConfirm, onCancel }: Props) {
  const summaryOperation =
    view.operations.find((operation) => operation.operation === "install" || operation.operation === "remove")
      ?.operation ?? view.operations[0]?.operation ?? "update";
  const details: ReactNode[] = [
    <p key="intro" className="asb-scope-note">
      应用前会再次校验目标文件未被外部改动；预览中的凭据值已脱敏。
      {view.operations.length > 1 && " 本预览包含多个资源，确认后将在同一事务中一起应用或一起回滚。"}
    </p>,
    ...view.operations.flatMap((operation, operationIndex) => [
      <p key={`resource-${operationIndex}`} className="asb-scope-note">
        资源 <span className="asb-code">{operation.definitionId}</span>（修订 r
        {operation.definitionRevision}）· {OPERATION_LABELS[operation.operation]}
      </p>,
      ...operation.targets.map((target, index) => (
        <div key={`${operationIndex}-${index}`} className="asb-ext-plan-target">
          <p>
            <strong>{targetLabel(target.target, projectNames)}</strong>
          </p>
          {target.warnings.map((warning) => (
            <p key={warning} className="asb-warn-text">
              警告：{warning}
            </p>
          ))}
          {target.changes.length > 0 && (
            <DiffView
              changes={target.changes.map(toKeyChange)}
              label={`${OPERATION_LABELS[operation.operation]}变更预览 ${index + 1}`}
            />
          )}
          {target.files !== undefined && target.files.length > 0 && (
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
      )),
    ]),
  ];

  return (
    <ConfirmSheet
      title={`${OPERATION_LABELS[summaryOperation]}预览`}
      details={details}
      confirmLabel="确认应用"
      confirmDisabled={busy}
      destructive={
        summaryOperation === "remove" || summaryOperation === "restore"
      }
      onConfirm={onConfirm}
      onCancel={onCancel}
    />
  );
}

export function ExtensionRemoveSheet({
  name,
  busy,
  onConfirm,
  onCancel,
}: {
  name: string;
  busy: boolean;
  onConfirm: () => void;
  onCancel: () => void;
}) {
  return (
    <ConfirmSheet
      title="删除扩展定义"
      details={[
        `将「${name}」从扩展库删除。`,
        "仍被部署绑定引用的定义无法删除；请先在客户端移除对应绑定。",
        "此操作不改动任何客户端配置文件。",
      ]}
      confirmLabel="确认删除"
      confirmDisabled={busy}
      destructive
      onConfirm={onConfirm}
      onCancel={onCancel}
    />
  );
}

/** Claude project Skills can place a visibility override in either the
 * shared project settings file or a person's local settings file. The
 * operation stays unplanned until the user makes that ownership choice. */
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
    <ConfirmSheet
      title="选择 Claude 项目 Skill 停用范围"
      details={[
        <p key="explanation">
          Claude 的项目 Skill 可见性规则可写入项目共享设置或个人本地设置。请明确选择本次规则的所有者。
        </p>,
        <label key="scope" className="asb-field">
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
        </label>,
      ]}
      confirmLabel="生成停用预览"
      confirmDisabled={busy || sharedSettings === null}
      onConfirm={onConfirm}
      onCancel={onCancel}
    />
  );
}
