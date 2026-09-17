import { useState } from "react";
import type { ExtensionListItem } from "../../api/client";
import { Button } from "../../components/Button";
import { Input } from "../../components/Input";
import { AppDialog } from "../../components/AppDialog";
import type { ExtensionWorkspace } from "./useExtensionWorkspace";

export function PortableExportSheet({
  workspace: w,
  item,
}: {
  workspace: ExtensionWorkspace;
  item: ExtensionListItem;
}) {
  const [path, setPath] = useState("");
  return (
    <AppDialog title={`导出便携包 ${item.name}`} busy={w.busy} onClose={w.nav.closeDialog}>
      <form
        className="asb-form"
        onSubmit={(event) => {
          event.preventDefault();
          if (path.trim())
            void w.ext.exportPortable(item.id, path.trim()).then((saved) => {
              if (saved) w.nav.closeDialog();
            });
        }}
      >
        <p className="asb-scope-note">便携包只包含库内容与非敏感来源信息；不含密钥、服务地址或本机路径。</p>
        <label className="asb-field">
          <span>导出文件路径</span>
          <Input
            required
            placeholder="D:\skills\docs-portable.json"
            value={path}
            disabled={w.busy}
            onChange={(event) => setPath(event.target.value)}
          />
        </label>
        <div className="asb-form-actions">
          <Button variant="secondary" disabled={w.busy} onClick={w.nav.closeDialog}>
            取消
          </Button>
          <Button type="submit" variant="primary" disabled={w.writeBlocked || !path.trim()}>
            导出到文件
          </Button>
        </div>
      </form>
    </AppDialog>
  );
}
