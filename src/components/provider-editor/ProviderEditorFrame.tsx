import type { ReactNode, Ref } from "react";
import { Button } from "../Button";
import { WorkspaceHeader } from "../WorkspaceHeader";

interface Props {
  title: string;
  titleRef?: Ref<HTMLHeadingElement>;
  backLabel: string;
  busy: boolean;
  onBack: () => void;
  onCancel: () => void;
  formId?: string;
  canSave: boolean;
  children: ReactNode;
}

/** Owns the fixed provider-editor frame: header, scroll body, and write bar. */
export function ProviderEditorFrame({
  title,
  titleRef,
  backLabel,
  busy,
  onBack,
  onCancel,
  formId,
  canSave,
  children,
}: Props) {
  const canSubmit = Boolean(formId) && canSave && !busy;
  return (
    <div className="asb-edit-view asb-provider-editor">
      <WorkspaceHeader title={title} titleRef={titleRef}
        back={<Button variant="back" className="asb-provider-editor-back" disabled={busy}
          aria-label={backLabel} onClick={onBack}>
          <span aria-hidden="true">←</span>{backLabel}
        </Button>} />
      <div className="asb-edit-panel">{children}</div>
      <footer className="asb-provider-form-footer">
        <Button variant="secondary" disabled={busy} onClick={onCancel}>取消</Button>
        <Button type={formId ? "submit" : "button"} form={formId} variant="primary" disabled={!canSubmit}>
          保存供应商
        </Button>
      </footer>
    </div>
  );
}
