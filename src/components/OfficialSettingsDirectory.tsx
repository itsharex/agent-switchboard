import type { OfficialSettingDirectoryEntry } from "../api/client";
import { useI18n } from "../i18n";
import { catalogText } from "../i18n/errors";

interface Props {
  entries: OfficialSettingDirectoryEntry[];
}

/**
 * A coverage map, not a second configuration editor. The backend directory
 * owns every path and its real write boundary; this component only makes that
 * boundary inspectable before a user expects a project, policy, or login
 * resource to be changed by a supplier activation.
 */
export function OfficialSettingsDirectory({ entries }: Props) {
  const { t } = useI18n();
  const dispositionLabel: Record<OfficialSettingDirectoryEntry["disposition"], string> = {
    direct: t("clientConfig.directory.dispositionDirect"),
    separateModule: t("clientConfig.directory.dispositionSeparate"),
    preserveOnly: t("clientConfig.directory.dispositionPreserve"),
  };
  return (
    <section className="asb-official-directory" aria-label={t("clientConfig.directory.title")}>
      <h3 className="asb-official-directory-title">{t("clientConfig.directory.title")}</h3>
      <div className="asb-official-directory-list">
        {entries.map((entry) => (
          <article className="asb-official-directory-entry" key={`${entry.title}:${entry.paths.join("|")}`}>
            <div className="asb-official-directory-entry-head">
              <h4 className="asb-group-title">{catalogText(entry.title, t)}</h4>
              <span className={`asb-official-directory-status is-${entry.disposition}`}>
                {dispositionLabel[entry.disposition]}
              </span>
            </div>
            <p className="asb-official-directory-paths">{entry.paths.map((path) => catalogText(path, t)).join(" · ")}</p>
            <p className="asb-official-directory-detail">{catalogText(entry.detail, t)}</p>
          </article>
        ))}
      </div>
    </section>
  );
}
