import type { GlobalPromptDocument } from "../api/client";
import { useI18n } from "../i18n";
import { Button } from "./Button";
import { Textarea } from "./Textarea";

interface GlobalPromptManagerProps {
  document: GlobalPromptDocument | undefined;
  draft: string;
  dirty: boolean;
  busy: boolean;
  onChange: (content: string) => void;
  onSave: () => void;
  onDiscard: () => void;
  onReload: () => void;
}

/**
 * Direct editor for the two supported user-global instruction files. It has
 * no prompt-template store: the file itself is the single source of truth.
 * The edited client follows the page-level client selection.
 */
export function GlobalPromptManager({
  document,
  draft,
  dirty,
  busy,
  onChange,
  onSave,
  onDiscard,
  onReload,
}: GlobalPromptManagerProps) {
  const { t } = useI18n();
  const fileName = document?.fileName ?? t("clientConfig.prompt.fallbackName");
  return (
    <>
      <label className="asb-prompt-editor-field">
        <span className="asb-prompt-file-name">{fileName}</span>
        <Textarea
          code
          aria-label={t("clientConfig.prompt.contentAria", { name: fileName })}
          value={draft}
          disabled={busy || !document}
          placeholder={document ? t("clientConfig.prompt.placeholder", { name: fileName }) : t("clientConfig.prompt.loadingPlaceholder")}
          onChange={(event) => onChange(event.target.value)}
        />
      </label>
      <div className="asb-prompt-actions">
        <Button
          variant="secondary"
          disabled={busy || dirty}
          onClick={onReload}
        >
          {t("clientConfig.common.reload")}
        </Button>
        {dirty && (
          <Button variant="secondary" disabled={busy} onClick={onDiscard}>
            {t("clientConfig.prompt.discardDraft")}
          </Button>
        )}
        <Button variant="primary" disabled={busy || !document || !dirty} onClick={onSave}>
          {busy ? t("clientConfig.prompt.saving") : t("clientConfig.prompt.save", { name: fileName })}
        </Button>
      </div>
    </>
  );
}
