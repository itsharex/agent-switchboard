import { Input } from "../../Input";
import { Select } from "../../Select";

interface Props {
  busy: boolean;
  optionsCwd: string;
  setOptionsCwd: (value: string) => void;
  optionsStartup: string;
  setOptionsStartup: (value: string) => void;
  optionsTool: string;
  setOptionsTool: (value: string) => void;
  optionsRequired: string;
  setOptionsRequired: (value: string) => void;
}

/** Optional Codex-side server options for one stdio MCP definition. */
export function CodexOptionsSection({
  busy,
  optionsCwd,
  setOptionsCwd,
  optionsStartup,
  setOptionsStartup,
  optionsTool,
  setOptionsTool,
  optionsRequired,
  setOptionsRequired,
}: Props) {
  return (
    <div className="asb-ext-section">
      <h4>Codex 选项（可选）</h4>
      <label className="asb-field">
        <span>工作目录</span>
        <Input
          placeholder="/srv/app"
          value={optionsCwd}
          disabled={busy}
          onChange={(event) => setOptionsCwd(event.target.value)}
        />
      </label>
      <label className="asb-field">
        <span>启动超时（秒）</span>
        <Input
          type="number"
          min={0}
          value={optionsStartup}
          disabled={busy}
          onChange={(event) => setOptionsStartup(event.target.value)}
        />
      </label>
      <label className="asb-field">
        <span>工具超时（秒）</span>
        <Input
          type="number"
          min={0}
          value={optionsTool}
          disabled={busy}
          onChange={(event) => setOptionsTool(event.target.value)}
        />
      </label>
      <div className="asb-field">
        <span>required</span>
        <Select
          value={optionsRequired}
          options={[
            { value: "", label: "（未设置）" },
            { value: "true", label: "true" },
            { value: "false", label: "false" },
          ]}
          onChange={(value) => setOptionsRequired(value)}
          ariaLabel="required"
          disabled={busy}
        />
      </div>
    </div>
  );
}
