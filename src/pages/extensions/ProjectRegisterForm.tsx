import { Button } from "../../components/Button";
import { FolderOpenIcon } from "../../components/icons";
import { Input } from "../../components/Input";

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
  return (
    <form
      className="asb-form"
      aria-label="注册项目目录"
      onSubmit={(event) => {
        event.preventDefault();
        void submitProject();
      }}
    >
      <label className="asb-field">
        <span>项目根目录（绝对路径）</span>
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
            浏览目录
          </Button>
        </div>
      </label>
      <div className="asb-form-actions">
        <Button type="submit" variant="primary" disabled={busy}>
          注册项目
        </Button>
      </div>
    </form>
  );
}
