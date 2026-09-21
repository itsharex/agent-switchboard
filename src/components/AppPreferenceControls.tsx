import type { ReactNode } from "react";
import { RadioOption } from "./RadioOption";

export function AppSettingRow({ label, detail, children }: {
  label: string;
  detail?: string;
  children: ReactNode;
}) {
  return (
    <div className="asb-app-setting-row">
      <div className="asb-app-setting-copy">
        <span className="asb-checkbox-label">{label}</span>
        {detail && <span className="asb-app-setting-detail">{detail}</span>}
      </div>
      {children}
    </div>
  );
}

export function AppSegmentSetting<T extends string | number>({ label, detail, value, options, busy, onChange }: {
  label: string;
  detail?: string;
  value: T;
  options: readonly { value: T; label: string }[];
  busy: boolean;
  onChange: (value: T) => void;
}) {
  return (
    <AppSettingRow label={label} detail={detail}>
      <div className="asb-segments" role="radiogroup" aria-label={label}>
        {options.map((option) => (
          <RadioOption key={option.value} name={label} label={option.label}
            checked={value === option.value} disabled={busy} onChange={() => onChange(option.value)} />
        ))}
      </div>
    </AppSettingRow>
  );
}
