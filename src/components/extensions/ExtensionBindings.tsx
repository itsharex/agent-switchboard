import type { ExtensionListItem, ProjectRegistration } from "../../api/client";
import { EXTENSION_CLIENTS, itemSupportsClient } from "../../app/extensions/deployment-state";
import { Button } from "../Button";
import { Checkbox } from "../Checkbox";
import type { SelectOption } from "../Select";
import type { ExtensionDetailProps } from "./detail-types";
import { FILE_STATE_LABELS, targetLabel, targetValue } from "./labels";

function installOptions(item: ExtensionListItem, projects: ProjectRegistration[]): SelectOption[] {
  const options = EXTENSION_CLIENTS.filter((client) => itemSupportsClient(item, client)).map((client) => ({
    value: `app:${client}`,
    label: targetLabel({ scope: "app", client }),
  }));
  for (const project of projects) {
    if (item.kind === "skill" && itemSupportsClient(item, "codex"))
      options.push({
        value: `projectShared:codex:${project.id}`,
        label: `${project.displayName} · Codex 项目共享`,
      });
    if (itemSupportsClient(item, "claude"))
      options.push(
        { value: `projectShared:claude:${project.id}`, label: `${project.displayName} · Claude 项目共享` },
        { value: `projectPrivate:claude:${project.id}`, label: `${project.displayName} · Claude 项目私有` },
      );
  }
  const existing = new Set(item.bindings.map((binding) => targetValue(binding.target)));
  return options.filter((option) => !existing.has(option.value));
}

export function ExtensionBindings({
  item,
  busy,
  projectNames,
  onChangeBinding,
  onToggleLock,
  onRemoveBinding,
}: ExtensionDetailProps) {
  return (
    <section className="asb-ext-section">
      <h4>已安装到</h4>
      {item.bindings.length === 0 ? (
        <p className="asb-empty">尚未部署到任何客户端</p>
      ) : (
        <ul className="asb-ext-binding-list">
          {item.bindings.map((binding) => (
            <li key={binding.id} className="asb-ext-binding">
              <Checkbox
                checked={binding.desired === "enabled"}
                label={targetLabel(binding.target, projectNames)}
                ariaLabel={`${binding.desired === "enabled" ? "停用" : "启用"} ${targetLabel(binding.target, projectNames)}`}
                disabled={busy}
                onChange={(checked) => onChangeBinding(binding, checked)}
              />
              <span className="asb-pill-status">{FILE_STATE_LABELS[binding.fileState]}</span>
              {binding.desired === "disabled" && <span className="asb-pill-status">已停用</span>}
              {binding.lockedDigest && (
                <span className="asb-pill-status">已固定 {binding.lockedDigest.slice(0, 12)}</span>
              )}
              {binding.warnings.map((warning) => (
                <span key={warning} className="asb-warn-text">
                  {warning}
                </span>
              ))}
              {item.kind === "skill" && (
                <Button
                  variant="secondary"
                  disabled={busy || (!binding.lockedDigest && binding.fileState !== "inSync")}
                  aria-label={`${binding.lockedDigest ? "解除固定" : "固定版本"} ${targetLabel(binding.target, projectNames)}`}
                  onClick={() => onToggleLock(binding, binding.lockedDigest == null)}
                >
                  {binding.lockedDigest ? "解除固定" : "固定版本"}
                </Button>
              )}
              <Button variant="secondary" disabled={busy} onClick={() => onRemoveBinding(binding)}>
                从客户端移除
              </Button>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}

export function ExtensionInstallTargets({
  item,
  projects,
  busy,
  installTargets,
  onInstallTargetsChange,
  onInstall,
}: ExtensionDetailProps) {
  const options = installOptions(item, projects);
  if (options.length === 0) return null;
  return (
    <section className="asb-ext-section">
      <h4>安装到其他位置</h4>
      <div className="asb-ext-target-list" aria-label="安装目标">
        {options.map((option) => (
          <Checkbox
            key={option.value}
            checked={installTargets.includes(option.value)}
            label={option.label}
            ariaLabel={`安装目标 ${option.label}`}
            disabled={busy}
            onChange={(checked) =>
              onInstallTargetsChange(
                checked
                  ? [...installTargets, option.value]
                  : installTargets.filter((value) => value !== option.value),
              )
            }
          />
        ))}
      </div>
      <div className="asb-ext-actions">
        <Button variant="primary" disabled={busy || installTargets.length === 0} onClick={onInstall}>
          安装到所选目标
        </Button>
      </div>
    </section>
  );
}
