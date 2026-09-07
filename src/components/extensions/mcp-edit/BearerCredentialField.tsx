import { Input } from "../../Input";
import { Select } from "../../Select";
import type { BearerDraft } from "./slots";

interface Props {
  busy: boolean;
  bearer: BearerDraft;
  setBearer: (next: BearerDraft) => void;
}

/** The optional HTTP bearer credential position of one MCP definition. */
export function BearerCredentialField({ busy, bearer, setBearer }: Props) {
  return (
    <div className="asb-ext-section">
      <div className="asb-ext-env-row">
        <span>Bearer 凭据</span>
        {bearer.kind === "keep" ? (
          <span className="asb-pill-status">已设置凭据（保持不变）</span>
        ) : bearer.kind === "none" ? (
          <span className="asb-scope-note">（不设置）</span>
        ) : (
          <Input
            aria-label="Bearer 凭据值"
            placeholder={bearer.kind === "envRef" ? "环境变量名" : "凭据值"}
            type={bearer.kind === "secret" ? "password" : "text"}
            autoComplete="off"
            value={bearer.text}
            disabled={busy}
            onChange={(event) => setBearer({ ...bearer, text: event.target.value })}
          />
        )}
        <Select
          value={bearer.kind}
          options={
            bearer.initial?.mode === "secretConfigured"
              ? [
                  { value: "keep", label: "保持已存凭据" },
                  { value: "none", label: "移除" },
                  { value: "plain", label: "明文值" },
                  { value: "envRef", label: "环境变量" },
                  { value: "secret", label: "新凭据" },
                ]
              : bearer.initial === null
                ? [
                    { value: "none", label: "不设置" },
                    { value: "plain", label: "明文值" },
                    { value: "envRef", label: "环境变量" },
                    { value: "secret", label: "新凭据" },
                  ]
                : [
                    { value: "none", label: "移除" },
                    { value: "plain", label: "明文值" },
                    { value: "envRef", label: "环境变量" },
                    { value: "secret", label: "新凭据" },
                  ]
          }
          onChange={(value) => setBearer({ ...bearer, kind: value as BearerDraft["kind"] })}
          ariaLabel="Bearer 凭据值类型"
          disabled={busy}
        />
      </div>
    </div>
  );
}
