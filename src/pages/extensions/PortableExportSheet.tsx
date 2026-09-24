import { useState } from "react";
import type { ExtensionListItem } from "../../api/client";
import { Button } from "../../components/Button";
import { Input } from "../../components/Input";
import { AppDialog } from "../../components/AppDialog";
import { useI18n } from "../../i18n";
import type { ExtensionWorkspace } from "./useExtensionWorkspace";

export function PortableExportSheet({
  workspace: w,
  item,
}: {
  workspace: ExtensionWorkspace;
  item: ExtensionListItem;
}) {
  const { t } = useI18n();
  const [path, setPath] = useState("");
  return (
    <AppDialog title={t("extensions.export.title", { name: item.name })} busy={w.busy} onClose={w.nav.closeDialog}>
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
        <p className="asb-scope-note">{t("extensions.export.note")}</p>
        <label className="asb-field">
          <span>{t("extensions.export.pathLabel")}</span>
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
            {t("confirm.cancel")}
          </Button>
          <Button type="submit" variant="primary" disabled={w.writeBlocked || !path.trim()}>
            {t("extensions.export.submit")}
          </Button>
        </div>
      </form>
    </AppDialog>
  );
}
