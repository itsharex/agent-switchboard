import { useState } from "react";
import { exportProvidersSql, type ProviderSqlExport } from "../../api/discovery";
import { pickDirectory, type CommandError } from "../../api/client";
import { Button } from "../../components/Button";
import { Input } from "../../components/Input";
import { ModuleHeader } from "../../components/WorkspaceHeader";
import { toast, toastMessage } from "../../components/use-toast";
import { useI18n } from "../../i18n";

/** File name filled in when a folder is picked and nothing is typed yet. */
const DEFAULT_EXPORT_FILE_NAME = "agent-switchboard-providers.sql";

/** Joins a picked folder with a file name, keeping the typed name when one
 * exists and following the folder's own path separator. */
function joinExportPath(directory: string, typedPath: string): string {
  const trimmed = directory.replace(/[\\/]+$/, "");
  const typedName = typedPath.trim().split(/[\\/]/).pop()?.trim();
  const fileName = typedName || DEFAULT_EXPORT_FILE_NAME;
  const separator = trimmed.includes("\\") ? "\\" : "/";
  return `${trimmed}${separator}${fileName}`;
}

interface SqlExportProps {
  busy: boolean;
  onError: (error: CommandError) => void;
}

function ExportResult({ result }: { result: ProviderSqlExport }) {
  const { t } = useI18n();
  return (
    <div className="asb-ccscan">
      <div
        className={`asb-banner ${result.skipped.length > 0 ? "asb-banner-warning" : "asb-banner-ok"}`}
        role="status"
        aria-label={t("importDiscovery.export.aria")}
      >
        <span>
          {t("importDiscovery.export.exported", { count: result.exportedCount })}
          {result.skipped.length > 0 && ` · ${t("importDiscovery.export.notExported", { count: result.skipped.length })}`}
        </span>
      </div>
      {result.skipped.map((skip, index) => (
        <div className="asb-kv" key={`${skip.name}-${index}`}>
          <span className="asb-kv-label">{skip.name}</span>
          <span className="asb-kv-value asb-warn-text">{skip.reason}</span>
        </div>
      ))}
    </div>
  );
}

/** Writes every stored provider document — complete configuration included —
 * into one SQL file. The companion 「导入 SQL」 panel applies the file on
 * another device without any command line; the file itself carries provider
 * credentials. */
export function SqlExport({ busy, onError }: SqlExportProps) {
  const { t } = useI18n();
  const [path, setPath] = useState("");
  const [working, setWorking] = useState(false);
  const [result, setResult] = useState<ProviderSqlExport | null>(null);
  const disabled = busy || working;
  const submit = async () => {
    const target = path.trim();
    if (!target) return;
    setWorking(true);
    try {
      const outcome = await exportProvidersSql(target);
      setResult(outcome);
      toast({
        kind: outcome.skipped.length > 0 ? "warning" : "success",
        title: toastMessage("importDiscovery.export.exported", { count: outcome.exportedCount }),
        description: target,
      });
    } catch (caught) {
      onError(caught as CommandError);
    } finally {
      setWorking(false);
    }
  };
  const pickFolder = async () => {
    if (disabled) return;
    const picked = await pickDirectory();
    if (picked) setPath((current) => joinExportPath(picked, current));
  };
  return (
    <>
      <ModuleHeader title={t("importDiscovery.tab.sqlExport")} />
      <form
        className="asb-form"
        onSubmit={(event) => {
          event.preventDefault();
          void submit();
        }}
      >
        <p className="asb-scope-note">
          {t("importDiscovery.export.note")}
        </p>
        <label className="asb-field">
          <span>{t("importDiscovery.export.pathLabel")}</span>
          <Input
            required
            placeholder={`D:\\providers\\${DEFAULT_EXPORT_FILE_NAME}`}
            value={path}
            disabled={disabled}
            onChange={(event) => setPath(event.target.value)}
          />
        </label>
        <div className="asb-form-actions">
          <Button variant="secondary" disabled={disabled} onClick={() => void pickFolder()}>
            {t("importDiscovery.export.pickFolder")}
          </Button>
          <Button type="submit" variant="primary" disabled={disabled || !path.trim()}>
            {working ? t("importDiscovery.export.working") : t("importDiscovery.export.toFile")}
          </Button>
        </div>
      </form>
      {result && <ExportResult result={result} />}
    </>
  );
}
