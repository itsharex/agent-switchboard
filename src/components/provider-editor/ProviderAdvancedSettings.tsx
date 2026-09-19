import type { ReactNode } from "react";
import { ChevronDownIcon } from "../icons";

interface Props {
  children: ReactNode;
}

/** Keeps optional provider configuration out of the connection-first path. */
export function ProviderAdvancedSettings({ children }: Props) {
  return (
    <section className="asb-editor-section asb-provider-advanced" aria-label="高级设置">
      <h3 className="asb-section-title">高级设置</h3>
      <div className="asb-editor-section-fields">
        <details className="asb-provider-disclosure">
          <summary><span>展开高级选项</span><span className="asb-provider-disclosure-value">按需配置</span><ChevronDownIcon /></summary>
          <div className="asb-provider-disclosure-body asb-provider-advanced-content">{children}</div>
        </details>
      </div>
    </section>
  );
}
