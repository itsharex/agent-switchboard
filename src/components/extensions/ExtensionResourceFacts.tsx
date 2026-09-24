import type { ReactNode } from "react";
import type { ExtensionListItem, SecretValueView as CredentialView } from "../../api/client";
import { useI18n } from "../../i18n";
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
  const { t } = useI18n();
  if (value.mode === "envRef")
    return (
      <span>
        {t("extensions.facts.envRef")} <span className="asb-code">{value.name}</span>
      </span>
    );
  return value.mode === "stored" ? (
    <span className="asb-pill-status">{t("extensions.facts.credentialSet")}</span>
  ) : (
    <span className="asb-code">••••••••</span>
  );
}

function SecretMap({ title, values }: { title: string; values: Record<string, CredentialView> }) {
  const { t } = useI18n();
  if (Object.keys(values).length === 0) return null;
  return (
    <div className="asb-ext-section">
      <h4 className="asb-group-title">{title}</h4>
      <ul className="asb-ext-secret-list">
        {Object.entries(values).map(([key, value]) => (
          <li key={key}>
            <span className="asb-code">{key}</span>
            {t("extensions.facts.entrySeparator")}
            <SecretValue value={value} />
          </li>
        ))}
      </ul>
    </div>
  );
}

function SkillFacts({ item }: { item: Extract<ExtensionListItem, { kind: "skill" }> }) {
  const { t } = useI18n();
  const rows: Array<[string, ReactNode]> = [
    [t("extensions.facts.skillName"), <span className="asb-code">{item.manifest.name}</span>],
  ];
  if (item.manifest.description) rows.push([t("extensions.facts.description"), item.manifest.description]);
  if (item.manifest.license) rows.push([t("extensions.facts.license"), item.manifest.license]);
  if (item.manifest.allowedTools?.length)
    rows.push([t("extensions.facts.allowedTools"), item.manifest.allowedTools.join(t("extensions.join.comma"))]);
  if (item.manifest.unparsedKeys?.length)
    rows.push([t("extensions.facts.unparsedKeys"), item.manifest.unparsedKeys.join(t("extensions.join.comma"))]);
  rows.push([t("extensions.facts.digest"), <span className="asb-code">{item.contentDigest.slice(0, 12)}</span>]);
  if (item.source)
    rows.push([
      t("extensions.facts.source"),
      item.source.resolvedCommit
        ? t("extensions.facts.sourceCommit", { commit: item.source.resolvedCommit.slice(0, 12) })
        : t("extensions.facts.sourceRecorded"),
    ]);
  if (item.hostScoped)
    rows.push([t("extensions.facts.hostScoped"), t("extensions.facts.hostOnly", { client: clientName(item.hostScoped) })]);
  const dependencyLabels = {
    bound: t("extensions.deps.bound"),
    pendingConfiguration: t("extensions.deps.pending"),
    targetUnsupported: t("extensions.deps.unsupported"),
  };
  return (
    <>
      <Facts rows={rows} />
      {item.compatibility.length > 0 && (
        <div className="asb-ext-section">
          <h4 className="asb-group-title">{t("extensions.facts.compatibility")}</h4>
          <ul className="asb-ext-secret-list">
            {item.compatibility.map((note) => (
              <li key={note.code}>{note.message}</li>
            ))}
          </ul>
        </div>
      )}
      <div className="asb-ext-section">
        <h4 className="asb-group-title">{t("extensions.facts.dependencies")}</h4>
        {item.dependencyStates.length === 0 ? (
          <p className="asb-empty">{t("extensions.facts.noDependencies")}</p>
        ) : (
          <ul className="asb-ext-secret-list">
            {item.dependencyStates.map((dependency) => (
              <li key={dependency.name}>
                {t("extensions.facts.dependencyLine", {
                  name: dependency.name,
                  state: dependencyLabels[dependency.state],
                })}
              </li>
            ))}
          </ul>
        )}
      </div>
    </>
  );
}

export function ExtensionResourceFacts({ item }: { item: ExtensionListItem }) {
  const { t } = useI18n();
  if (item.kind === "skill") return <SkillFacts item={item} />;
  const rows: Array<[string, ReactNode]> = [[t("extensions.facts.transport"), t(TRANSPORT_LABELS[item.transport])]];
  if (item.transport === "stdio")
    rows.push(
      [t("extensions.facts.command"), <span className="asb-code">{item.command}</span>],
      [
        t("extensions.facts.args"),
        item.argumentCount > 0
          ? t("extensions.facts.argsCount", { count: item.argumentCount })
          : t("extensions.facts.none"),
      ],
    );
  else rows.push([t("extensions.facts.url"), <span className="asb-code">{item.url}</span>]);
  return (
    <>
      <Facts rows={rows} />
      {item.transport === "stdio" ? (
        <SecretMap title={t("extensions.facts.envVars")} values={item.env} />
      ) : (
        <SecretMap title={t("extensions.facts.headers")} values={item.headers} />
      )}
      {item.transport === "http" && item.bearer && (
        <div className="asb-ext-section">
          <h4 className="asb-group-title">{t("extensions.facts.bearer")}</h4>
          <SecretValue value={item.bearer} />
        </div>
      )}
    </>
  );
}
