import type { ReactNode } from "react";
import { useI18n } from "../../i18n";
import { ChevronDownIcon } from "../icons";

interface Props {
  children: ReactNode;
}

/** Keeps optional provider configuration out of the connection-first path. */
export function ProviderAdvancedSettings({ children }: Props) {
  const { t } = useI18n();
  return (
    <section className="asb-editor-section asb-provider-advanced" aria-label={t("providers.editor.section.advanced")}>
      <h3 className="asb-section-title">{t("providers.editor.section.advanced")}</h3>
      <div className="asb-editor-section-fields">
        <details className="asb-provider-disclosure">
          <summary><span>{t("providers.editor.advanced.expand")}</span><span className="asb-provider-disclosure-value">{t("providers.editor.advanced.onDemand")}</span><ChevronDownIcon /></summary>
          <div className="asb-provider-disclosure-body asb-provider-advanced-content">{children}</div>
        </details>
      </div>
    </section>
  );
}
