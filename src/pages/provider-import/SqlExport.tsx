import { useState } from "react";
import { exportProvidersSql, type ProviderSqlExport } from "../../api/discovery";
import { pickDirectory, type CommandError } from "../../api/client";
import { Button } from "../../components/Button";
import { Input } from "../../components/Input";
import { ModuleHeader } from "../../components/WorkspaceHeader";
import { toast } from "../../components/use-toast";

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
  return (
    <div className="asb-ccscan">
      <div
        className={`asb-banner ${result.skipped.length > 0 ? "asb-banner-warning" : "asb-banner-ok"}`}
        role="status"
        aria-label="导出结果"
      >
        <span>
          已导出 {result.exportedCount} 项供应商
          {result.skipped.length > 0 && ` · 未导出 ${result.skipped.length} 项`}
        </span>
      </div>
      {result.skipped.map((skip, index) => (
        <div className="asb-kv" key={`${skip.name}-${index}`}>
          <span className="asb-kv-label">{skip.name}</span>
          <span className="asb-kv-value asb-warn-text">未导出：{skip.reason}</span>
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
        title: `已导出 ${outcome.exportedCount} 项供应商`,
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
      <ModuleHeader title="导出 SQL" />
      <form
        className="asb-form"
        onSubmit={(event) => {
          event.preventDefault();
          void submit();
        }}
      >
        <p className="asb-scope-note">
          将全部供应商（Claude、Codex 第三方与官方登录记录）的完整配置导出为一个 SQL
          文件；在其他设备打开「导入 / 导出 → 导入 SQL」选择该文件即可导入，配置完整还原，无需命令行。文件包含
          API 密钥，请妥善保管。
        </p>
        <label className="asb-field">
          <span>导出文件路径</span>
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
            选择文件夹
          </Button>
          <Button type="submit" variant="primary" disabled={disabled || !path.trim()}>
            {working ? "正在导出…" : "导出到文件"}
          </Button>
        </div>
      </form>
      {result && <ExportResult result={result} />}
    </>
  );
}
