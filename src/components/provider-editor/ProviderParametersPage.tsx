import type { SettingsValues, SettingValue } from "../../api/client";
import { Button } from "../Button";
import { SettingsFields } from "../SettingsFields";
import type { useProviderParameters } from "./useProviderParameters";

interface Props {
  value: SettingsValues | null;
  onChange: (value: SettingsValues) => void;
  parameters: ReturnType<typeof useProviderParameters>;
  busy: boolean;
  baselineValues?: Record<string, SettingValue>;
}

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

export function ProviderParametersPage({ value, onChange, parameters, busy, baselineValues }: Props) {
  const catalog = parameters.catalog;
  if (!catalog || !value) return <ParametersLoadStatus busy={busy}
    ready={parameters.ready} error={parameters.error} retry={parameters.retry} />;
  const baseline = baselineValues ?? catalog.defaults.settings;
  const resetAll = () => onChange({ settings: { ...catalog.defaults.settings } });
  const change = (key: string, next: SettingValue) =>
    onChange({ settings: { ...value.settings, [key]: next } });
  return (
    <div className="asb-form asb-provider-parameters" aria-label="供应商运行参数">
      <p className="asb-field-help">运行参数随此供应商保存；当前设置来自已保存的供应商档案，新建供应商从默认值开始。修改会在返回编辑后统一保存。</p>
      <SettingsFields specs={catalog.specs} groups={catalog.groups} values={value.settings}
        baselineValues={baseline} busy={busy} showGroupReset={false} presentation="provider" onChange={change} />
      <div className="asb-settings-actions">
        <Button variant="secondary" disabled={busy} onClick={resetAll}>恢复运行参数默认值</Button>
      </div>
    </div>
  );
}
