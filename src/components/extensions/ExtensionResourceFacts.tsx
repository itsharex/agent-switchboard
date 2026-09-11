import type { ReactNode } from "react";
import type { ExtensionListItem, SecretValueView as CredentialView } from "../../api/client";
import { clientName } from "../../lib/client-name";
import { TRANSPORT_LABELS } from "./labels";

function Facts({ rows }: { rows: Array<[string, ReactNode]> }) {
  return (
    <dl className="asb-fact-row">
      {rows.map(([label, content]) => (
        <div key={label}>
          <dt>{label}</dt>
          <dd>{content}</dd>
        </div>
      ))}
    </dl>
  );
}

function SecretValue({ value }: { value: CredentialView }) {
  if (value.mode === "envRef")
    return (
      <span>
        环境变量 <span className="asb-code">{value.name}</span>
      </span>
    );
  return value.mode === "stored" ? (
    <span className="asb-pill-status">已设置凭据</span>
  ) : (
    <span className="asb-code">••••••••</span>
  );
}

function SecretMap({ title, values }: { title: string; values: Record<string, CredentialView> }) {
  if (Object.keys(values).length === 0) return null;
  return (
    <div className="asb-ext-section">
      <h4 className="asb-group-title">{title}</h4>
      <ul className="asb-ext-secret-list">
        {Object.entries(values).map(([key, value]) => (
          <li key={key}>
            <span className="asb-code">{key}</span>：<SecretValue value={value} />
          </li>
        ))}
      </ul>
    </div>
  );
}

function SkillFacts({ item }: { item: Extract<ExtensionListItem, { kind: "skill" }> }) {
  const rows: Array<[string, ReactNode]> = [
    ["Skill 名称", <span className="asb-code">{item.manifest.name}</span>],
  ];
  if (item.manifest.description) rows.push(["描述", item.manifest.description]);
  if (item.manifest.license) rows.push(["许可", item.manifest.license]);
  if (item.manifest.allowedTools?.length) rows.push(["允许工具", item.manifest.allowedTools.join("、")]);
  if (item.manifest.unparsedKeys?.length)
    rows.push(["未识别的清单键", item.manifest.unparsedKeys.join("、")]);
  rows.push(["内容摘要", <span className="asb-code">{item.contentDigest.slice(0, 12)}</span>]);
  if (item.source)
    rows.push([
      "来源",
      `已记录来源${item.source.resolvedCommit ? ` @ ${item.source.resolvedCommit.slice(0, 12)}` : ""}`,
    ]);
  if (item.hostScoped) rows.push(["宿主专用", `仅 ${clientName(item.hostScoped)}`]);
  const dependencyLabels = {
    bound: "已关联库内 MCP",
    pendingConfiguration: "待配置",
    targetUnsupported: "目标客户端不支持",
  };
  return (
    <>
      <Facts rows={rows} />
      {item.compatibility.length > 0 && (
        <div className="asb-ext-section">
          <h4 className="asb-group-title">兼容诊断</h4>
          <ul className="asb-ext-secret-list">
            {item.compatibility.map((note) => (
              <li key={note.code}>{note.message}</li>
            ))}
          </ul>
        </div>
      )}
      <div className="asb-ext-section">
        <h4 className="asb-group-title">依赖状态</h4>
        {item.dependencyStates.length === 0 ? (
          <p className="asb-empty">未声明依赖</p>
        ) : (
          <ul className="asb-ext-secret-list">
            {item.dependencyStates.map((dependency) => (
              <li key={dependency.name}>
                {dependency.name}：{dependencyLabels[dependency.state]}
              </li>
            ))}
          </ul>
        )}
      </div>
    </>
  );
}

export function ExtensionResourceFacts({ item }: { item: ExtensionListItem }) {
  if (item.kind === "skill") return <SkillFacts item={item} />;
  const rows: Array<[string, ReactNode]> = [["传输", TRANSPORT_LABELS[item.transport]]];
  if (item.transport === "stdio")
    rows.push(
      ["命令", <span className="asb-code">{item.command}</span>],
      ["参数", item.argumentCount > 0 ? `已配置 ${item.argumentCount} 个启动参数` : "（无）"],
    );
  else rows.push(["地址", <span className="asb-code">{item.url}</span>]);
  return (
    <>
      <Facts rows={rows} />
      {item.transport === "stdio" ? (
        <SecretMap title="环境变量" values={item.env} />
      ) : (
        <SecretMap title="请求头" values={item.headers} />
      )}
      {item.transport === "http" && item.bearer && (
        <div className="asb-ext-section">
          <h4 className="asb-group-title">Bearer 凭据</h4>
          <SecretValue value={item.bearer} />
        </div>
      )}
    </>
  );
}
