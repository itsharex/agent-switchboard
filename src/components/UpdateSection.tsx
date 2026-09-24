import type { ReactNode } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import type { UpdateChannel, UpdateCheck } from "../api/client";
import type { UpdateDownloadProgress } from "../app/useUpdateCheck";
import { Button } from "./Button";
import { Time } from "./Time";
import { useI18n } from "../i18n";
import { tr } from "../i18n/current";
import { toast, toastMessage } from "./use-toast";

const releasesUrl = "https://github.com/y4Nkk/agent-switchboard/releases/latest";

function ReleaseNotes({ notes, version }: { notes: string; version: string }) {
  const { t } = useI18n();
  const sections = notes
    .split(/\r?\n(?=### )/)
    .map((section) => {
      const [heading, ...lines] = section.trim().split(/\r?\n/);
      return {
        title: heading.replace(/^###\s+/, ""),
        items: lines
          .map((line) => line.replace(/^-\s+/, "").trim())
          .filter(Boolean),
      };
    })
    .filter((section) => section.title && section.items.length > 0);

  if (sections.length === 0) {
    return <p className="asb-update-notes-text">{notes}</p>;
  }
  return (
    <div className="asb-update-notes" aria-label={t("backup.update.notesAria", { version })}>
      {sections.map((section) => (
        <section key={section.title} className="asb-update-notes-section">
          <h4 className="asb-section-title">{section.title}</h4>
          <ul>{section.items.map((item) => <li key={item}>{item}</li>)}</ul>
        </section>
      ))}
    </div>
  );
}

function formatProgress(progress: UpdateDownloadProgress): string {
  if (progress.totalBytes === null || progress.totalBytes === 0) {
    return tr("backup.update.progressBytes", { kb: Math.floor(progress.downloadedBytes / 1024) });
  }
  return tr("backup.update.progressPercent", {
    percent: Math.min(100, Math.floor((progress.downloadedBytes / progress.totalBytes) * 100)),
  });
}

/** Software-update state on the settings page. Startup checks are silent;
 * download, verification and installation are explicit user actions. */
export function UpdateSection({
  channel,
  appVersion,
  result,
  busy,
  installing,
  progress,
  checkedAt,
  restartRequired,
  onCheck,
  onInstall,
  onRestart,
}: {
  channel: UpdateChannel | null;
  /** Running build's version; null until the process reports it. */
  appVersion: string | null;
  result: UpdateCheck | null;
  busy: boolean;
  installing: boolean;
  progress: UpdateDownloadProgress | null;
  checkedAt: string | null;
  restartRequired: boolean;
  onCheck: () => void;
  onInstall: () => void;
  onRestart: () => void;
}) {
  const { t } = useI18n();
  const openReleasePage = () => {
    void openUrl(releasesUrl).catch(() => {
      toast({
        kind: "error",
        title: toastMessage("backup.update.releasePageError"),
        description: toastMessage("backup.update.releasePageErrorDetail"),
      });
    });
  };

  if (channel === "microsoftStore") {
    return (
      <section className="asb-app-settings-group" aria-labelledby="software-update">
        <h3 id="software-update" className="asb-section-title">
          {t("backup.update.title")}
        </h3>
        <div className="asb-app-setting-row">
          <div className="asb-app-setting-copy">
            <span className="asb-checkbox-label">{t("backup.update.storeManaged")}</span>
            <span className="asb-app-setting-detail">
              {[
                appVersion ? t("backup.update.currentVersion", { version: appVersion }) : null,
                t("backup.update.storeAuto"),
              ]
                .filter(Boolean)
                .join(" · ")}
            </span>
          </div>
        </div>
      </section>
    );
  }
  const label = restartRequired
    ? installing
      ? t("backup.update.restarting")
      : t("backup.update.restartRequired")
    : result
    ? installing
      ? progress
        ? formatProgress(progress)
        : t("backup.update.installing")
      : t("backup.update.newVersion", { version: result.latestVersion })
    : busy
      ? t("backup.update.checking")
      : checkedAt
        ? t("backup.update.upToDate")
      : t("backup.update.check");
  const detail: ReactNode = appVersion || checkedAt ? (
    <>
      {appVersion ? t("backup.update.currentVersion", { version: appVersion }) : null}
      {appVersion && checkedAt ? " · " : null}
      {checkedAt ? (
        <>
          {t("backup.update.checkedAt")} <Time iso={checkedAt} />
        </>
      ) : null}
    </>
  ) : null;
  return (
    <section className="asb-app-settings-group" aria-labelledby="software-update">
      <h3 id="software-update" className="asb-section-title">
        {t("backup.update.title")}
      </h3>
      <div className="asb-app-setting-row">
        <div className="asb-app-setting-copy">
          <span className="asb-checkbox-label">{label}</span>
          {detail ? <span className="asb-app-setting-detail">{detail}</span> : null}
          {result?.releaseNotes ? <ReleaseNotes notes={result.releaseNotes} version={result.latestVersion} /> : null}
        </div>
        <div className="asb-panel-actions">
          {restartRequired ? (
            <Button variant="primary" disabled={installing} onClick={onRestart}>
              {t("backup.update.restart")}
            </Button>
          ) : result ? (
            <Button
              variant="primary"
              disabled={busy || installing}
              onClick={onInstall}
            >
              {t("backup.update.downloadInstall")}
            </Button>
          ) : null}
          <Button variant="secondary" disabled={busy || installing || restartRequired} onClick={onCheck}>
            {t("backup.update.checkUpdate")}
          </Button>
          <Button variant="secondary" onClick={openReleasePage}>
            {t("backup.update.releasePage")}
          </Button>
        </div>
      </div>
    </section>
  );
}
