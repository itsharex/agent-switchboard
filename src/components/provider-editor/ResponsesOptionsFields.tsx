import { useState } from "react";
import type { ResponsesOptions } from "../../api/client";
import { ChevronDownIcon } from "../icons";
import { Select } from "../Select";

interface Props {
  options: ResponsesOptions | null;
  busy: boolean;
  onChange: (next: ResponsesOptions) => void;
}

export function ResponsesOptionsFields({ options, busy, onChange }: Props) {
  const [expanded, setExpanded] = useState(() => !options || options.requestMode === "minimal");
  const minimal = options?.requestMode === "minimal";
  return (
    <details className="asb-provider-disclosure" open={expanded}
      onToggle={(event) => setExpanded(event.currentTarget.open)}>
      <summary><span>Responses 能力</span><span className="asb-provider-disclosure-value">
        {!options ? "配置不完整" : `HTTP/SSE · ${minimal ? "最小请求" : "标准请求"}`}
      </span><ChevronDownIcon /></summary>
      {!options ? <p role="alert" className="asb-scope-note asb-warn-text">此 Responses 档案缺少能力配置，请重新选择 API 格式后保存。</p>
        : <div className="asb-provider-disclosure-body asb-provider-field-grid">
      <label className="asb-field">
        <span>请求模式</span>
        <Select ariaLabel="请求模式" value={options.requestMode} disabled={busy}
          options={[{ value: "standard", label: "标准请求" }, { value: "minimal", label: "最小请求" }]}
          onChange={(value) => {
            const requestMode = value as ResponsesOptions["requestMode"];
            onChange({ requestMode });
          }} />
        <p className="asb-scope-note">
          {minimal
            ? "经本机网关移除可选扩展字段，保留输入与工具语义；网关先还原 previous_response_id 引用的上下文，再发送完整输入；引用失效时明确报错。此模式不发送 store，数据保留遵循供应商默认策略。"
            : "保留客户端发送的 Responses 字段；供应商不支持可选扩展字段时，可选择最小请求。"}
        </p>
      </label>
      <p className="asb-scope-note">第三方统一使用 HTTP/SSE；Codex 与本机网关之间可使用 WebSocket，无需供应商开关。</p>
      </div>}
    </details>
  );
}
