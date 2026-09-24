import { FileJson } from "lucide-react";
import { Button } from "../../components/Button";
import { Input } from "../../components/Input";
import { useI18n } from "../../i18n";

interface Props {
  busy: boolean;
  portablePath: string;
  setPortablePath: (value: string) => void;
  browsePortable: () => void | Promise<void>;
  submitPortableImport: () => void | Promise<void>;
}

/** Imports one key-free portable extension package from a local file. */
export function PortableImportForm({
  busy,
  portablePath,
  setPortablePath,
  browsePortable,
  submitPortableImport,
}: Props) {
  const { t } = useI18n();
  return (
    <form
      className="asb-form"
      aria-label={t("extensions.toolbar.importPortable")}
      onSubmit={(event) => {
        event.preventDefault();
        void submitPortableImport();
      }}
    >
      <label className="asb-field">
        <span>{t("extensions.import.pathLabel")}</span>
        <div className="asb-field-input-row">
          <Input
            required
            placeholder="D:\\skills\\docs.asbskill.json"
            value={portablePath}
            disabled={busy}
            onChange={(event) => setPortablePath(event.target.value)}
          />
          <Button variant="secondary" disabled={busy} onClick={() => void browsePortable()}>
            <FileJson size={16} />
            {t("extensions.import.pick")}
          </Button>
        </div>
      </label>
      <div className="asb-form-actions">
        <Button type="submit" variant="primary" disabled={busy}>
          {t("extensions.import.submit")}
        </Button>
      </div>
    </form>
  );
}
