import { Button } from "../../components/Button";
import { Input } from "../../components/Input";

interface Props {
  busy: boolean;
  portablePath: string;
  setPortablePath: (value: string) => void;
  submitPortableImport: () => void | Promise<void>;
}

/** Imports one key-free portable extension package from a local file. */
export function PortableImportForm({
  busy,
  portablePath,
  setPortablePath,
  submitPortableImport,
}: Props) {
  return (
    <form
      className="asb-form"
      aria-label="导入便携包"
      onSubmit={(event) => {
        event.preventDefault();
        void submitPortableImport();
      }}
    >
      <label className="asb-field">
        <span>便携包文件路径（.json，导出的内容不含任何密钥）</span>
        <Input
          required
          placeholder="D:\\skills\\docs.asbskill.json"
          value={portablePath}
          disabled={busy}
          onChange={(event) => setPortablePath(event.target.value)}
        />
      </label>
      <div className="asb-form-actions">
        <Button type="submit" variant="primary" disabled={busy}>
          导入便携包
        </Button>
      </div>
    </form>
  );
}
