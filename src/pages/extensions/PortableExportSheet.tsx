import type { ExtensionListItem } from "../../api/client";
import { Button } from "../../components/Button";
import { Input } from "../../components/Input";

interface Props {
  busy: boolean;
  exportTarget: ExtensionListItem;
  exportPath: string;
  setExportPath: (value: string) => void;
  submitPortableExport: () => void | Promise<void>;
  closeExport: () => void;
}

/** Exports one library entry as a key-free portable package file. */
export function PortableExportSheet({
  busy,
  exportTarget,
  exportPath,
  setExportPath,
  submitPortableExport,
  closeExport,
}: Props) {
  return (
    <div className="asb-sheet-backdrop">
      <form
        className="asb-sheet"
        role="dialog"
        aria-modal="true"
        aria-label={`导出便携包 ${exportTarget.name}`}
        onSubmit={(event) => {
          event.preventDefault();
          void submitPortableExport();
        }}
      >
        <h2 className="asb-panel-title">导出便携包 {exportTarget.name}</h2>
        <ul className="asb-sheet-details">
          <li>
            便携包只包含库内容与非敏感来源信息；不含密钥、服务地址或本机路径
            {exportTarget.kind === "mcp" && exportTarget.transport !== "stdio"
              ? "。远程 MCP 无法导出"
              : ""}
          </li>
        </ul>
        <label className="asb-field">
          <span>导出文件路径</span>
          <Input
            required
            placeholder="D:\\skills\\docs-portable.json"
            value={exportPath}
            disabled={busy}
            onChange={(event) => setExportPath(event.target.value)}
          />
        </label>
        <div className="asb-sheet-actions">
          <Button
            type="button"
            variant="secondary"
            disabled={busy}
            onClick={closeExport}
          >
            取消
          </Button>
          <Button type="submit" variant="primary" disabled={busy || !exportPath.trim()}>
            导出到文件
          </Button>
        </div>
      </form>
    </div>
  );
}
