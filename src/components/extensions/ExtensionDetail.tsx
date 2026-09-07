import type { ReactNode } from "react";
import type {
  BindingStatus,
  ClientCapabilityReport,
  ExtensionListItem,
  ProjectRegistration,
  SecretValueView,
  SkillUpdateReport,
} from "../../api/client";
import { clientName } from "../../lib/client-name";
import { Button } from "../Button";
import { Checkbox } from "../Checkbox";
import { type SelectOption } from "../Select";
import { Time } from "../Time";
import { CapabilityPanel } from "./CapabilityPanel";
import { ExtensionCheckPanel } from "./ExtensionCheckPanel";
import {
  FILE_STATE_LABELS,
  SKILL_CHANGE_LABELS,
  TRANSPORT_LABELS,
  mcpSupportsClient,
  parseTargetValue,
  targetLabel,
} from "./labels";

interface Props {
  item: ExtensionListItem;
  projects: ProjectRegistration[];
  capabilities: ClientCapabilityReport[];
  busy: boolean;
  /** Encoded install targets the user has checked; empty means none. */
  installTargets: string[];
  updateReport: SkillUpdateReport | null;
  projectNames: ReadonlyMap<string, string>;
  onInstallTargetsChange: (values: string[]) => void;
  onInstall: () => void;
  onChangeBinding: (binding: BindingStatus, enable: boolean) => void;
  onRemoveBinding: (binding: BindingStatus) => void;
  onToggleLock: (binding: BindingStatus, locked: boolean) => void;
  onDelete: () => void;
  onEditMcp: () => void;
  onEditSkillContent?: () => void;
  onCheckUpdates: () => void;
  onApplyUpdate: () => void;
  onDeployCurrent: () => void;
  /** Opens the portable-export flow; absent when export is unavailable. */
  onExportPortable?: () => void;
  onClose: () => void;
}

function Facts({ rows }: { rows: Array<[string, ReactNode]> }) {
  return (
    <dl className="asb-ext-facts">
      {rows.map(([label, content]) => (
        <div key={label}>
          <dt>{label}</dt>
          <dd>{content}</dd>
        </div>
      ))}
    </dl>
  );
}

function SecretValueView({ value }: { value: SecretValueView }) {
  if (value.mode === "envRef") {
    return (
      <span>
        环境变量 <span className="asb-code">{value.name}</span>
      </span>
    );
  }
  if (value.mode === "stored") return <span className="asb-pill-status">已设置凭据</span>;
  return <span className="asb-code">••••••••</span>;
}

function SecretMapSection({
  title,
  entries,
}: {
  title: string;
  entries: Array<[string, SecretValueView]>;
}) {
  if (entries.length === 0) return null;
  return (
    <div className="asb-ext-section">
      <h4>{title}</h4>
      <ul className="asb-ext-secret-list">
        {entries.map(([key, value]) => (
          <li key={key}>
            <span className="asb-code">{key}</span>：<SecretValueView value={value} />
          </li>
        ))}
      </ul>
    </div>
  );
}

function secretEntries(
  values: Record<string, SecretValueView> | undefined,
): Array<[string, SecretValueView]> {
  return Object.entries(values ?? {});
}

function dependencyStateLabel(state: ExtensionListItem["dependencyStates"][number]["state"]): string {
  if (state === "bound") return "已关联库内 MCP";
  if (state === "pendingConfiguration") return "待配置";
  return "目标客户端不支持";
}

/** Installable targets for this definition: app scopes always, project
 * scopes follow each client's real document model (Codex has no private
 * project document, and its project MCP servers live in user config). */
function buildTargetOptions(
  item: ExtensionListItem,
  projects: ProjectRegistration[],
): SelectOption[] {
  const options: SelectOption[] = [];
  const allowClient = (client: "codex" | "claude") => {
    if (item.kind === "skill") {
      return (item.hostScoped ?? null) === null || item.hostScoped === client;
    }
    return mcpSupportsClient(item.transport, client);
  };
  if (allowClient("codex")) {
    options.push({ value: "app:codex", label: "Codex 用户配置" });
  }
  if (allowClient("claude")) {
    options.push({ value: "app:claude", label: "Claude 用户配置" });
  }
  for (const project of projects) {
    if (item.kind === "skill" && allowClient("codex")) {
      options.push({
        value: `projectShared:codex:${project.id}`,
        label: `${project.displayName} · Codex 项目共享`,
      });
    }
    if (allowClient("claude")) {
      options.push({
        value: `projectShared:claude:${project.id}`,
        label: `${project.displayName} · Claude 项目共享`,
      });
      options.push({
        value: `projectPrivate:claude:${project.id}`,
        label: `${project.displayName} · Claude 项目私有`,
      });
    }
  }
  return options;
}

/** One library definition's full workspace detail: facts, bindings, and the
 * per-target operation entry points. */
export function ExtensionDetail({
  item,
  projects,
  capabilities,
  busy,
  installTargets,
  updateReport,
  projectNames,
  onInstallTargetsChange,
  onInstall,
  onChangeBinding,
  onRemoveBinding,
  onToggleLock,
  onDelete,
  onEditMcp,
  onEditSkillContent,
  onCheckUpdates,
  onApplyUpdate,
  onDeployCurrent,
  onExportPortable,
  onClose,
}: Props) {
  const targetOptions = buildTargetOptions(item, projects);
  const checkTarget =
    installTargets.length > 0 ? parseTargetValue(installTargets[0]) : null;

  return (
    <section className="asb-ext-detail" aria-label={`扩展详情 ${item.name}`}>
      <div className="asb-ext-detail-main">
        <header className="asb-ext-detail-head">
          <h3>{item.name}</h3>
          <span className="asb-pill-status">{item.kind === "skill" ? "Skill" : "MCP"}</span>
          <Button variant="secondary" onClick={onClose}>
            关闭详情
          </Button>
        </header>
        <p className="asb-scope-note">
          修订 r{item.revision} · 更新于 <Time iso={item.updatedAt} /> · 标识 <span className="asb-code">{item.id}</span>
        </p>

        {item.kind === "skill" ? (
          <>
            <Facts
              rows={[
                ["Skill 名称", <span className="asb-code">{item.manifest.name}</span>],
                ...(item.manifest.description ? ([["描述", item.manifest.description]] as Array<[string, ReactNode]>) : []),
                ...(item.manifest.license ? ([["许可", item.manifest.license]] as Array<[string, ReactNode]>) : []),
                ...(item.manifest.allowedTools && item.manifest.allowedTools.length > 0
                  ? ([["允许工具", item.manifest.allowedTools.join("、")]] as Array<[string, ReactNode]>)
                  : []),
                ...(item.manifest.unparsedKeys && item.manifest.unparsedKeys.length > 0
                  ? ([["未识别的清单键", item.manifest.unparsedKeys.join("、")]] as Array<[string, ReactNode]>)
                  : []),
                ["内容摘要", <span className="asb-code">{item.contentDigest.slice(0, 12)}</span>],
                ...(item.source
                  ? ([
                      [
                        "来源",
                        <span>
                          <span className="asb-pill-status">已记录来源</span>
                          {item.source.resolvedCommit ? ` @ ${item.source.resolvedCommit.slice(0, 12)}` : ""}
                        </span>,
                      ],
                    ] as Array<[string, ReactNode]>)
                  : []),
                ...(item.hostScoped ? ([["宿主专用", `仅 ${clientName(item.hostScoped)}`]] as Array<[string, ReactNode]>) : []),
              ]}
            />
            {(item.compatibility?.length ?? 0) > 0 && (
              <div className="asb-ext-section">
                <h4>兼容诊断</h4>
                <ul className="asb-ext-secret-list">
                  {item.compatibility?.map((note) => (
                    <li key={note.code}>
                      <span className="asb-code">{note.code}</span>：{note.message}
                    </li>
                  ))}
                </ul>
              </div>
            )}
            <div className="asb-ext-section">
              <h4>依赖状态</h4>
              {item.dependencyStates.length === 0 ? (
                <p className="asb-empty">未声明依赖</p>
              ) : (
                <ul className="asb-ext-secret-list">
                  {item.dependencyStates.map((dependency) => (
                    <li key={dependency.name}>
                      {dependency.name}：{dependencyStateLabel(dependency.state)}
                    </li>
                  ))}
                </ul>
              )}
            </div>
          </>
        ) : (
          <>
            <Facts
              rows={[
                ["传输", TRANSPORT_LABELS[item.transport] ?? item.transport],
                ...(item.transport === "stdio"
                  ? ([
                      ["命令", <span className="asb-code">{item.command}</span>],
                      ["参数", item.argumentCount > 0 ? `已配置 ${item.argumentCount} 个启动参数` : "（无）"],
                    ] as Array<[string, ReactNode]>)
                  : ([
                      ["地址", <span className="asb-code">{item.url}</span>],
                    ] as Array<[string, ReactNode]>)),
              ]}
            />
            {item.transport === "stdio" ? (
              <SecretMapSection title="环境变量" entries={secretEntries(item.env)} />
            ) : (
              <SecretMapSection title="请求头" entries={secretEntries(item.headers)} />
            )}
            {item.transport === "http" && item.bearer && (
              <div className="asb-ext-section">
                <h4>Bearer 凭据</h4>
                <SecretValueView value={item.bearer} />
              </div>
            )}
          </>
        )}

        <div className="asb-ext-section">
          <h4>目标绑定</h4>
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
                    <span className="asb-pill-status">
                      已固定 {binding.lockedDigest.slice(0, 12)}
                    </span>
                  )}
                  {binding.warnings.map((warning) => (
                    <span key={warning} className="asb-warn-text">
                      {warning}
                    </span>
                  ))}
                  {item.kind === "skill" && (
                    <Button
                      variant="secondary"
                      disabled={busy}
                      aria-label={`${
                        binding.lockedDigest ? "解除固定" : "固定版本"
                      } ${targetLabel(binding.target, projectNames)}`}
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
        </div>

        <div className="asb-ext-section">
          <h4>扩展操作</h4>
          <div className="asb-ext-actions">
            {targetOptions.length > 0 ? (
              <>
                <div className="asb-ext-target-list" aria-label="安装目标">
                  {targetOptions.map((option) => (
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
                <Button
                  variant="primary"
                  disabled={busy || installTargets.length === 0}
                  onClick={onInstall}
                >
                  安装到所选目标
                </Button>
              </>
            ) : (
              <p className="asb-empty">没有适用于该扩展的安装目标</p>
            )}
            {item.kind === "skill" && (
              <Button variant="secondary" disabled={busy} onClick={onCheckUpdates}>
                检查更新
              </Button>
            )}
            {item.kind === "skill" && onEditSkillContent && (
              <Button variant="secondary" disabled={busy} onClick={onEditSkillContent}>
                编辑内容
              </Button>
            )}
            {item.kind === "mcp" && (
              <Button variant="secondary" disabled={busy} onClick={onEditMcp}>
                编辑定义
              </Button>
            )}
            {onExportPortable && (
              <Button variant="secondary" disabled={busy} onClick={onExportPortable}>
                导出便携包
              </Button>
            )}
            {item.bindings.some(
              (binding) =>
                binding.desired === "enabled" && binding.fileState === "pendingApply",
            ) && (
              <Button variant="secondary" disabled={busy} onClick={onDeployCurrent}>
                预览部署当前版本
              </Button>
            )}
            <Button variant="danger" disabled={busy} onClick={onDelete}>
              删除定义
            </Button>
          </div>
          {item.kind === "skill" && updateReport && (
            <p className="asb-scope-note">
              {updateReport.upToDate ? (
                "来源内容已是最新"
              ) : (
                <>
                  发现新版本（提交 {updateReport.newCommit?.slice(0, 12) ?? "未知"}）{" "}
                  <Button
                    variant="secondary"
                    disabled={busy || !updateReport.newDigest}
                    onClick={onApplyUpdate}
                  >
                    更新到新内容
                  </Button>
                </>
              )}
            </p>
          )}
          {item.kind === "skill" && updateReport !== null && updateReport.changedFiles.length > 0 && (
            <div className="asb-ext-section">
              <h4>与来源的文件差异</h4>
              <ul className="asb-ext-file-list">
                {updateReport.changedFiles.map((file) => (
                  <li key={`${file.action}-${file.relativePath}`} className="asb-code">
                    {SKILL_CHANGE_LABELS[file.action]} {file.relativePath}
                  </li>
                ))}
              </ul>
            </div>
          )}
        </div>

        {item.kind === "mcp" && (
          <ExtensionCheckPanel definitionId={item.id} target={checkTarget} busy={busy} />
        )}
      </div>
      <CapabilityPanel reports={capabilities} />
    </section>
  );
}
