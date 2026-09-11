import { useId } from "react";
import type { AppKind } from "../api/client";
import { clientName } from "../lib/client-name";
import { ClientLogo } from "./ClientLogo";

export function ClientPicker({ app, onChange, disabled, label }: {
  app: AppKind;
  onChange: (app: AppKind) => void;
  disabled: boolean;
  label: string;
}) {
  const name = useId();
  return (
    <div className="asb-segments asb-client-picker" role="radiogroup" aria-label={label}>
      {(["codex", "claude"] as const).map((value) => (
        <label key={value} className={`asb-seg-opt${app === value ? " is-active" : ""}`}>
          <input type="radio" name={name} checked={app === value} disabled={disabled}
            onChange={() => onChange(value)} />
          <ClientLogo app={value} className="asb-seg-logo" />
          {clientName(value)}
        </label>
      ))}
    </div>
  );
}
