import { CommandErrorLines } from "../app/notifications";
import { openUrl } from "@tauri-apps/plugin-opener";
import { type MouseEvent, useEffect, useRef, useState } from "react";
import {
  getCloudBackupSetupSql,
  type CloudBackupSettings,
  type CommandError,
} from "../api/client";
import { Button } from "./Button";
import { ConfirmSheet } from "./ConfirmSheet";
import { Input } from "./Input";
import { Textarea } from "./Textarea";
import { useI18n } from "../i18n";
import { toast, toastMessage } from "./use-toast";

type PendingOperation = "upload" | "restore" | null;

const SUPABASE_DASHBOARD_URL = "https://supabase.com/dashboard";

interface SupabaseDashboardLinks {
  project: string;
  dataApi: string;
  sqlEditor: string;
  authUsers: string;
}

function supabaseDashboardLinks(projectUrl: string): SupabaseDashboardLinks {
  const fallback = {
    project: SUPABASE_DASHBOARD_URL,
    dataApi: SUPABASE_DASHBOARD_URL,
    sqlEditor: SUPABASE_DASHBOARD_URL,
    authUsers: SUPABASE_DASHBOARD_URL,
  };
  try {
    const url = new URL(projectUrl.trim());
    const suffix = ".supabase.co";
    if (url.protocol !== "https:" || !url.hostname.endsWith(suffix)) return fallback;
    const projectRef = url.hostname.slice(0, -suffix.length);
    if (!projectRef || projectRef.includes(".")) return fallback;
    const project = `${SUPABASE_DASHBOARD_URL}/project/${encodeURIComponent(projectRef)}`;
    return {
      project,
      dataApi: `${project}/integrations/data_api/overview`,
      sqlEditor: `${project}/sql/new`,
      authUsers: `${project}/auth/users`,
    };
  } catch {
    return fallback;
  }
}

interface Props {
  settings: CloudBackupSettings | null;
  loaded: boolean;
  busy: boolean;
  onSave: (settings: CloudBackupSettings) => Promise<boolean>;
  onTestConnection: (settings: CloudBackupSettings, accountPassword: string) => Promise<boolean>;
  onUpload: (accountPassword: string, backupPassword: string) => Promise<boolean>;
  onRestore: (accountPassword: string, backupPassword: string) => Promise<boolean>;
}

/** User-configured, encrypted profile-store backup. Passwords remain in
 * component state only and clear after a successful remote operation. */
export function CloudBackupPanel({
  settings,
  loaded,
  busy,
  onSave,
  onTestConnection,
  onUpload,
  onRestore,
}: Props) {
  const { t } = useI18n();
  const [draft, setDraft] = useState<CloudBackupSettings>({
    projectUrl: "",
    publishableKey: "",
    email: "",
  });
  const [accountPassword, setAccountPassword] = useState("");
  const [backupPassword, setBackupPassword] = useState("");
  const [pending, setPending] = useState<PendingOperation>(null);
  const [setupSql, setSetupSql] = useState<string | null>(null);
  const [guideOpen, setGuideOpen] = useState(false);
  const connectionForm = useRef<HTMLFormElement>(null);
  const dashboardLinks = supabaseDashboardLinks(draft.projectUrl);

  useEffect(() => {
    if (settings) setDraft(settings);
  }, [settings]);

  const revealSetupSql = () => {
    if (setupSql !== null) {
      setSetupSql(null);
      return;
    }
    void getCloudBackupSetupSql()
      .then(setSetupSql)
      .catch((caught) => {
        const error = caught as CommandError;
        toast({ kind: "error", title: toastMessage("backup.cloud.setupSqlError"), description: <CommandErrorLines error={error} /> });
      });
  };

  const confirm = async () => {
    if (pending === "upload") {
      const succeeded = await onUpload(accountPassword, backupPassword);
      if (succeeded) {
        setAccountPassword("");
        setBackupPassword("");
        setPending(null);
      }
      return;
    }
    if (pending === "restore") {
      const succeeded = await onRestore(accountPassword, backupPassword);
      if (succeeded) {
        setAccountPassword("");
        setBackupPassword("");
        setPending(null);
      }
    }
  };

  const testConnection = async () => {
    if (!connectionForm.current?.reportValidity()) return;
    await onTestConnection(draft, accountPassword);
  };

  const copySetupSql = async () => {
    if (setupSql === null) return;
    try {
      await navigator.clipboard.writeText(setupSql);
      toast({ kind: "success", title: toastMessage("backup.cloud.setupSqlCopied") });
    } catch {
      toast({ kind: "error", title: toastMessage("backup.cloud.setupSqlCopyFailed") });
    }
  };

  const openGuideLink = (event: MouseEvent<HTMLAnchorElement>) => {
    event.preventDefault();
    void openUrl(event.currentTarget.href);
  };

  return (
    <section className="asb-cloud-backup">
      <section className="asb-cloud-backup-guide" aria-labelledby="cloud-backup-guide-title">
        <div className="asb-cloud-backup-guide-heading">
          <h3 id="cloud-backup-guide-title" className="asb-cloud-backup-guide-title">
            {t("backup.cloud.guideTitle")}
          </h3>
          <Button
            variant="secondary"
            aria-expanded={guideOpen}
            aria-controls="cloud-backup-guide-content"
            onClick={() => setGuideOpen((open) => !open)}
          >
            {guideOpen ? t("backup.cloud.guideCollapse") : t("backup.cloud.guideExpand")}
          </Button>
        </div>
        <div id="cloud-backup-guide-content" hidden={!guideOpen}>
          <ol className="asb-cloud-backup-guide-list">
            <li>
              <h4 className="asb-group-title">{t("backup.cloud.guide.create.title")}</h4>
              <p>
                {t("backup.cloud.guide.create.before")}
                <a className="asb-cloud-backup-guide-link" href={SUPABASE_DASHBOARD_URL} onClick={openGuideLink}>Supabase Dashboard</a>
                {t("backup.cloud.guide.create.after")}
              </p>
            </li>
            <li>
              <h4 className="asb-group-title">{t("backup.cloud.guide.connect.title")}</h4>
              <p>
                {t("backup.cloud.guide.connect.before")}
                <a className="asb-cloud-backup-guide-link" href={dashboardLinks.project} onClick={openGuideLink}>{t("backup.cloud.guide.connect.projectLink")}</a>
                {t("backup.cloud.guide.connect.mid1")}
                <code className="asb-code">Connect</code>
                {t("backup.cloud.guide.connect.mid2")}
                <code className="asb-code">Project URL</code>
                {t("backup.cloud.guide.connect.mid3")}
                <code className="asb-code">Publishable key</code>
                {t("backup.cloud.guide.connect.mid4")}
                <code className="asb-code">Access Token</code>
                {t("backup.cloud.guide.connect.mid5")}
                <code className="asb-code">Secret key</code>
                {t("backup.cloud.guide.connect.mid6")}
                <code className="asb-code">service_role</code>
                {t("backup.cloud.guide.connect.after")}
              </p>
            </li>
            <li>
              <h4 className="asb-group-title">{t("backup.cloud.guide.dataApi.title")}</h4>
              <p>
                {t("backup.cloud.guide.dataApi.before")}
                <a className="asb-cloud-backup-guide-link" href={dashboardLinks.dataApi} onClick={openGuideLink}>Integrations → Data API</a>
                {t("backup.cloud.guide.dataApi.mid1")}
                <code className="asb-code">Enable Data API</code>
                {t("backup.cloud.guide.dataApi.mid2")}
                <a className="asb-cloud-backup-guide-link" href={dashboardLinks.sqlEditor} onClick={openGuideLink}>SQL Editor</a>
                {t("backup.cloud.guide.dataApi.after")}
              </p>
            </li>
            <li>
              <h4 className="asb-group-title">{t("backup.cloud.guide.auth.title")}</h4>
              <p>
                {t("backup.cloud.guide.auth.before")}
                <a className="asb-cloud-backup-guide-link" href={dashboardLinks.authUsers} onClick={openGuideLink}>Authentication → Users</a>
                {t("backup.cloud.guide.auth.mid1")}
                <code className="asb-code">Add user → Create new user</code>
                {t("backup.cloud.guide.auth.mid2")}
                <code className="asb-code">Auto Confirm User</code>
                {t("backup.cloud.guide.auth.after")}
              </p>
            </li>
            <li>
              <h4 className="asb-group-title">{t("backup.cloud.guide.fill.title")}</h4>
              <p>{t("backup.cloud.guide.fill.body")}</p>
            </li>
            <li>
              <h4 className="asb-group-title">{t("backup.cloud.guide.save.title")}</h4>
              <p>{t("backup.cloud.guide.save.body")}</p>
            </li>
          </ol>
        </div>
      </section>
      {!loaded ? (
        <div className="asb-cloud-backup-loading" role="status" aria-label={t("backup.cloud.loadingAria")}>
          <div className="asb-skeleton" />
          <div className="asb-skeleton" />
          <div className="asb-skeleton" />
        </div>
      ) : (
        <>
          <form
            ref={connectionForm}
            className="asb-form"
            aria-label={t("backup.cloud.formAria")}
            onSubmit={(event) => {
              event.preventDefault();
              void onSave(draft);
            }}
          >
            <label className="asb-field">
              <span>{t("backup.cloud.projectUrl")}</span>
              <Input
                type="url"
                required
                placeholder="https://your-project.supabase.co"
                value={draft.projectUrl}
                disabled={busy}
                onChange={(event) =>
                  setDraft((current) => ({ ...current, projectUrl: event.target.value }))
                }
              />
            </label>
            <label className="asb-field">
              <span>Publishable key</span>
              <Input
                type="password"
                required
                autoComplete="off"
                value={draft.publishableKey}
                disabled={busy}
                onChange={(event) =>
                  setDraft((current) => ({ ...current, publishableKey: event.target.value }))
                }
              />
            </label>
            <label className="asb-field">
              <span>{t("backup.cloud.authEmail")}</span>
              <Input
                type="email"
                required
                autoComplete="username"
                value={draft.email}
                disabled={busy}
                onChange={(event) =>
                  setDraft((current) => ({ ...current, email: event.target.value }))
                }
              />
            </label>
            <label className="asb-field">
              <span>{t("backup.cloud.authPassword")}</span>
              <Input
                type="password"
                autoComplete="current-password"
                value={accountPassword}
                disabled={busy}
                onChange={(event) => setAccountPassword(event.target.value)}
              />
            </label>
            <div className="asb-form-actions">
              <Button variant="secondary" disabled={busy} onClick={revealSetupSql}>
                {setupSql === null ? t("backup.cloud.showSetupSql") : t("backup.cloud.hideSetupSql")}
              </Button>
              {setupSql !== null && (
                <Button variant="secondary" disabled={busy} onClick={() => void copySetupSql()}>
                  {t("backup.cloud.copySetupSql")}
                </Button>
              )}
              <Button
                variant="secondary"
                disabled={busy || accountPassword.length === 0}
                onClick={() => void testConnection()}
              >
                {t("backup.cloud.testConnection")}
              </Button>
              <Button type="submit" variant="primary" disabled={busy}>
                {t("backup.cloud.saveConnection")}
              </Button>
            </div>
          </form>
          {setupSql !== null && (
            <label className="asb-field">
              <span>{t("backup.cloud.setupSqlLabel")}</span>
              <Textarea
                readOnly
                aria-label={t("backup.cloud.setupSqlAria")}
                value={setupSql}
              />
            </label>
          )}
          <fieldset className="asb-fieldset">
            <legend>{t("backup.cloud.backupOrRestore")}</legend>
            <label className="asb-field">
              <span>{t("backup.cloud.backupPassword")}</span>
              <Input
                type="password"
                autoComplete="new-password"
                minLength={8}
                value={backupPassword}
                disabled={busy}
                onChange={(event) => setBackupPassword(event.target.value)}
              />
            </label>
            <div className="asb-form-actions">
              <Button
                variant="primary"
                disabled={busy || settings === null}
                onClick={() => setPending("upload")}
              >
                {t("backup.cloud.upload")}
              </Button>
            </div>
            <div className="asb-backup-danger-row">
              <Button
                variant="danger"
                disabled={busy || settings === null}
                onClick={() => setPending("restore")}
              >
                {t("backup.cloud.restore")}
              </Button>
            </div>
          </fieldset>
        </>
      )}
      {pending === "upload" && (
        <ConfirmSheet
          title={t("backup.cloud.uploadConfirmTitle")}
          confirmLabel={t("backup.cloud.uploadConfirmConfirm")}
          onConfirm={() => void confirm()}
          onCancel={() => setPending(null)}
        >
          <ul className="asb-dialog-details">
            <li>{t("backup.cloud.uploadConfirm1")}</li>
            <li>{t("backup.cloud.uploadConfirm2")}</li>
            <li>{t("backup.cloud.uploadConfirm3")}</li>
          </ul>
        </ConfirmSheet>
      )}
      {pending === "restore" && (
        <ConfirmSheet
          title={t("backup.cloud.restoreConfirmTitle")}
          confirmLabel={t("backup.confirmRestore")}
          destructive
          onConfirm={() => void confirm()}
          onCancel={() => setPending(null)}
        >
          <ul className="asb-dialog-details">
            <li>{t("backup.cloud.restoreConfirm1")}</li>
            <li>{t("backup.cloud.restoreConfirm2")}</li>
            <li>{t("backup.cloud.restoreConfirm3")}</li>
          </ul>
        </ConfirmSheet>
      )}
    </section>
  );
}
