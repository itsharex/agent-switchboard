import type { SettingValue } from "../../api/client";
import { Button } from "../Button";
import { SettingsFields } from "../SettingsFields";
import type { ProviderEditorState } from "./useProviderEditor";

interface Props { editor: ProviderEditorState; busy: boolean }

export function ParametersLoadStatus({ ready, error, retry, busy }: {
  ready: boolean;
  error: string | null;
  retry: () => void;
  busy: boolean;
}) {
  if (ready) return null;
  return error ? (
    <div className="asb-field-error" role="alert">
      <p>无法读取运行参数：{error}</p>
      <Button variant="secondary" disabled={busy} onClick={retry}>重新读取运行参数</Button>
    </div>
  ) : <p className="asb-field-help" role="status">正在读取运行参数</p>;
}

export function ProviderParametersPage({ editor, busy }: Props) {
  const { draft, setDraft, parameters } = editor;
  const catalog = parameters.catalog;
  if (!catalog || !draft.parameters) return <ParametersLoadStatus busy={busy}
    ready={parameters.ready} error={parameters.error} retry={parameters.retry} />;
  const reset = (group: string | null) => setDraft((current) => {
    if (!current.parameters) return current;
    const settings = { ...current.parameters.settings };
    for (const spec of catalog.specs) {
      if (group === null || spec.group === group) settings[spec.key] = catalog.defaults.settings[spec.key];
    }
    return { ...current, parameters: { settings } };
  });
  const change = (key: string, value: SettingValue) => setDraft((current) => current.parameters
    ? { ...current, parameters: { settings: { ...current.parameters.settings, [key]: value } } }
    : current);
  return (
    <div className="asb-form" aria-label="供应商运行参数">
      <p className="asb-field-help">运行参数随此供应商保存，返回编辑后统一保存供应商。</p>
      <SettingsFields specs={catalog.specs} groups={catalog.groups} values={draft.parameters.settings} busy={busy}
        onResetGroup={reset} onChange={change} />
      <div className="asb-settings-actions">
        <Button variant="secondary" disabled={busy} onClick={() => reset(null)}>全部恢复默认值</Button>
      </div>
    </div>
  );
}
