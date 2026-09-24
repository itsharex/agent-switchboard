import { Button } from "../../components/Button";
import { FolderOpenIcon } from "../../components/icons";
import { Input } from "../../components/Input";
import { useI18n } from "../../i18n";

interface Props {
  busy: boolean;
  projectRoot: string;
  setProjectRoot: (value: string) => void;
  browseProject: () => void | Promise<void>;
  submitProject: () => void | Promise<void>;
}

/** Registers one project directory as an extension deployment target. */
export function ProjectRegisterForm({
  busy,
  projectRoot,
  setProjectRoot,
  browseProject,
  submitProject,
}: Props) {
  const { t } = useI18n();
  return (
    <form
      className="asb-form"
      aria-label={t("extensions.toolbar.registerProject")}
      onSubmit={(event) => {
        event.preventDefault();
        void submitProject();
      }}
    >
      <label className="asb-field">
        <span>{t("extensions.project.rootLabel")}</span>
        <div className="asb-field-input-row">
          <Input
            required
            placeholder="D:\\works\\my-project"
            value={projectRoot}
            disabled={busy}
            onChange={(event) => setProjectRoot(event.target.value)}
          />
          <Button variant="secondary" disabled={busy} onClick={() => void browseProject()}>
            <FolderOpenIcon />
            {t("extensions.sources.browse")}
          </Button>
        </div>
      </label>
      <div className="asb-form-actions">
        <Button type="submit" variant="primary" disabled={busy}>
          {t("extensions.project.submit")}
        </Button>
      </div>
    </form>
  );
}
