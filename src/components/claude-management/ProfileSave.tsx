import type { ProfileSavePreparation } from "../../api/providers";
import { Button } from "../Button";
import { PreviewInspector } from "../PreviewInspector";
export function ProfileSave({ preparation, busy, onConfirm, onCancel }: {
  preparation: ProfileSavePreparation; busy: boolean; onConfirm: () => void; onCancel: () => void;
}) {
  return <section aria-label="Claude 档案保存确认" className="asb-provider-section-fields">
    <p className="asb-scope-note">{preparation.kind === "saveAndApply"
      ? "此档案正在使用。确认后会备份并更新 Claude 配置与本机路由；外部改动会使本次预览失效。"
      : "只保存 Claude 本地档案，不启用供应商，不改写 Claude 客户端文件。"}</p>
    {preparation.preview && <PreviewInspector filePreview={preparation.preview} userConfigModel={null} userConfigWarnings={[]} />}
    <div className="asb-form-actions"><Button variant="secondary" disabled={busy} onClick={onCancel}>取消保存</Button>
      <Button variant="primary" disabled={busy} onClick={onConfirm}>{preparation.kind === "saveAndApply" ? "确认保存并应用 Claude 配置" : "确认保存 Claude 档案"}</Button></div>
  </section>;
}
