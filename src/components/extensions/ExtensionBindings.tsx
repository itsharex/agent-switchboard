import type { ExtensionListItem, ProjectRegistration } from "../../api/client";
import { EXTENSION_CLIENTS, itemSupportsClient } from "../../app/extensions/deployment-state";
import { useI18n } from "../../i18n";
import { tr } from "../../i18n/current";
import { Button } from "../Button";
import { ConnectivityIcon, PinIcon, TrashIcon } from "../icons";
import { Tooltip } from "../Tooltip";
import { Checkbox } from "../Checkbox";
import type { SelectOption } from "../Select";
import type { ExtensionManagementProps } from "./management-types";
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
        label: tr("extensions.install.projectSharedCodex", { project: project.displayName }),
      });
    if (itemSupportsClient(item, "claude"))
      options.push(
        { value: `projectShared:claude:${project.id}`, label: tr("extensions.install.projectSharedClaude", { project: project.displayName }) },
        { value: `projectPrivate:claude:${project.id}`, label: tr("extensions.install.projectPrivateClaude", { project: project.displayName }) },
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
}: ExtensionManagementProps) {
  const { t } = useI18n();
  return (
    <section className="asb-ext-section">
      <h4 className="asb-section-title">{t("extensions.bindings.title")}</h4>
      {item.bindings.length === 0 ? (
        <div className="asb-empty-state">
          <span className="asb-empty-state-icon" aria-hidden="true">
            <ConnectivityIcon />
          </span>
          <h3 className="asb-section-title">{t("extensions.bindings.empty")}</h3>
        </div>
      ) : (
        <ul className="asb-ext-binding-list">
          {item.bindings.map((binding) => (
            <li key={binding.id} className="asb-ext-binding">
              <Checkbox
                checked={binding.desired === "enabled"}
                label={targetLabel(binding.target, projectNames)}
                ariaLabel={`${t(binding.desired === "enabled" ? "extensions.bindings.disableAction" : "extensions.bindings.enableAction")} ${targetLabel(binding.target, projectNames)}`}
                disabled={busy}
                onChange={(checked) => onChangeBinding(binding, checked)}
              />
              <span className="asb-pill-status">{t(FILE_STATE_LABELS[binding.fileState])}</span>
              {binding.desired === "disabled" && <span className="asb-pill-status">{t("extensions.bindings.disabled")}</span>}
              {binding.lockedDigest && (
                <span className="asb-pill-status">{t("extensions.bindings.pinned", { digest: binding.lockedDigest.slice(0, 12) })}</span>
              )}
              {binding.warnings.map((warning) => (
                <span key={warning} className="asb-warn-text">
                  {warning}
                </span>
              ))}
              <div className="asb-ext-binding-actions">
                {item.kind === "skill" && (
                  <Tooltip label={t(binding.lockedDigest ? "extensions.bindings.unpin" : "extensions.bindings.pin")}>
                    <Button
                      variant="icon"
                      className="asb-ext-binding-action"
                      disabled={busy || (!binding.lockedDigest && binding.fileState !== "inSync")}
                      aria-label={`${t(binding.lockedDigest ? "extensions.bindings.unpinAction" : "extensions.bindings.pinAction")} ${targetLabel(binding.target, projectNames)}`}
                      onClick={() => onToggleLock(binding, binding.lockedDigest == null)}
                    >
                      <PinIcon />
                    </Button>
                  </Tooltip>
                )}
                <Tooltip label={t("extensions.bindings.remove")}>
                  <Button
                    variant="icon"
                    className="asb-ext-binding-action is-danger"
                    disabled={busy}
                    aria-label={`${t("extensions.bindings.remove")} ${targetLabel(binding.target, projectNames)}`}
                    onClick={() => onRemoveBinding(binding)}
                  >
                    <TrashIcon />
                  </Button>
                </Tooltip>
              </div>
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
}: ExtensionManagementProps) {
  const { t } = useI18n();
  const options = installOptions(item, projects);
  if (options.length === 0) return null;
  return (
    <section className="asb-ext-section">
      <details className="asb-ext-disclosure asb-ext-install-targets">
        <summary>
          <span className="asb-section-title">{t("extensions.install.title")}</span>
          <span className="asb-scope-note">{t("extensions.install.count", { count: options.length })}</span>
        </summary>
        <div className="asb-ext-disclosure-content">
          <div className="asb-ext-target-list" aria-label={t("extensions.install.listAria")}>
            {options.map((option) => (
              <Checkbox
                key={option.value}
                checked={installTargets.includes(option.value)}
                label={option.label}
                ariaLabel={`${t("extensions.install.listAria")} ${option.label}`}
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
          <Button variant="primary" disabled={busy || installTargets.length === 0} onClick={onInstall}>
            {t("extensions.install.selected")}
          </Button>
        </div>
      </details>
    </section>
  );
}
