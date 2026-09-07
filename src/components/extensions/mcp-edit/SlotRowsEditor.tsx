import type { MutableRefObject } from "react";
import { Button } from "../../Button";
import { Input } from "../../Input";
import { Select } from "../../Select";
import { slotKindOptions, type SlotDraft, type SlotKind } from "./slots";

interface Props {
  busy: boolean;
  rows: SlotDraft[];
  setRows: (next: SlotDraft[]) => void;
  label: string;
  keyPlaceholder: string;
  nextRowId: MutableRefObject<number>;
}

/** One editable list of named secret-bearing positions (env vars or headers). */
export function SlotRowsEditor({ busy, rows, setRows, label, keyPlaceholder, nextRowId }: Props) {
  return (
    <div className="asb-ext-section">
      {rows.map((row, index) => (
        <div key={row.id} className="asb-ext-env-row">
          <Input
            aria-label={`${label}名 ${index + 1}`}
            placeholder={keyPlaceholder}
            value={row.name}
            disabled={busy || row.kind === "keep"}
            onChange={(event) =>
              setRows(rows.map((current, i) => (i === index ? { ...current, name: event.target.value } : current)))
            }
          />
          {row.kind === "keep" ? (
            <span className="asb-pill-status">已设置凭据（保持不变）</span>
          ) : (
            <Input
              aria-label={`${label}值 ${index + 1}`}
              placeholder={row.kind === "envRef" ? "环境变量名" : row.kind === "secret" ? "凭据值" : "值"}
              type={row.kind === "secret" ? "password" : "text"}
              autoComplete="off"
              value={row.text}
              disabled={busy}
              onChange={(event) =>
                setRows(rows.map((current, i) => (i === index ? { ...current, text: event.target.value } : current)))
              }
            />
          )}
          <Select
            value={row.kind}
            options={slotKindOptions(row.initial)}
            onChange={(value) =>
              setRows(rows.map((current, i) => (i === index ? { ...current, kind: value as SlotKind } : current)))
            }
            ariaLabel={`${label} ${index + 1} 值类型`}
            disabled={busy}
          />
          <Button
            variant="secondary"
            disabled={busy}
            aria-label={`移除${label} ${index + 1}`}
            onClick={() => setRows(rows.filter((_, i) => i !== index))}
          >
            删除
          </Button>
        </div>
      ))}
      <Button
        variant="secondary"
        disabled={busy}
        onClick={() =>
          setRows([
            ...rows,
            {
              id: nextRowId.current++,
              name: "",
              kind: "plain",
              text: "",
              initial: null,
            },
          ])
        }
      >
        添加{label}
      </Button>
    </div>
  );
}
