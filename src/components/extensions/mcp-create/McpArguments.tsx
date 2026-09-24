import { useRef } from "react";
import { useI18n } from "../../../i18n";
import { Button } from "../../Button";
import { Textarea } from "../../Textarea";
import { Tooltip } from "../../Tooltip";
import { PlusIcon, TrashIcon } from "../../icons";
import type { ArgumentRow } from "./wizard-state";

interface Props {
  rows: ArgumentRow[];
  busy: boolean;
  onChange: (rows: ArgumentRow[]) => void;
}

export function McpArguments({ rows, busy, onChange }: Props) {
  const { t } = useI18n();
  const nextId = useRef(Math.max(0, ...rows.map(({ id }) => id)) + 1);
  return (
    <div className="asb-mcp-wizard-section" role="group" aria-label={t("mcp.field.args")}>
      <div className="asb-mcp-field-heading">
        <span>{t("mcp.field.args")}</span>
        <Tooltip label={t("mcp.args.add")}>
          <Button variant="icon" disabled={busy} aria-label={t("mcp.args.add")}
            onClick={() => onChange([...rows, { id: nextId.current++, value: "" }])}><PlusIcon /></Button>
        </Tooltip>
      </div>
      {rows.map((row, index) => (
        <div key={row.id} className="asb-mcp-argument">
          <span className="asb-num asb-mcp-argument-index" aria-hidden="true">{index + 1}</span>
          <Textarea code rows={1} aria-label={t("mcp.args.rowAria", { index: index + 1 })} value={row.value} disabled={busy}
            onChange={(event) => onChange(rows.map((item) => item.id === row.id ? { ...item, value: event.target.value } : item))} />
          <Tooltip label={t("mcp.args.remove", { index: index + 1 })}>
            <Button variant="icon" disabled={busy} aria-label={t("mcp.args.remove", { index: index + 1 })}
              onClick={() => onChange(rows.filter(({ id }) => id !== row.id))}><TrashIcon /></Button>
          </Tooltip>
        </div>
      ))}
    </div>
  );
}
