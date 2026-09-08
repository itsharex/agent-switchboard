import { useState } from "react";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type {
  CodexSubagentSettings,
  CodexSubagentSettingsPreview,
  CodexSubagentSettingsSnapshot,
  SettingValue,
} from "../api/client";
import type { CodexSubagentSettingsEditorState } from "../app/useCodexSubagentSettings";
import { CodexSubagentSettingsPanel } from "./CodexSubagentSettingsPanel";

const automatic: CodexSubagentSettings = {
  enabled: { mode: "automatic" },
  maxConcurrentThreadsPerSession: { mode: "automatic" },
  interruptMessage: { mode: "automatic" },
};

const preview: CodexSubagentSettingsPreview = {
  app: "codex",
  target: "Codex 用户级配置",
  content: "[agents]\nenabled = true\n",
  configHash: "config-hash",
  renderedHash: "candidate-hash",
};

function editorState(
  settings: CodexSubagentSettings = automatic,
  deprecatedKeys: string[] = [],
): CodexSubagentSettingsEditorState {
  const snapshot: CodexSubagentSettingsSnapshot = {
    app: "codex",
    settings,
    configHash: "config-hash",
    fileExists: true,
    deprecatedKeys,
  };
  return { phase: "clean", snapshot, draft: settings };
}

function Harness({ initial = editorState(), onApply = vi.fn() }: {
  initial?: CodexSubagentSettingsEditorState;
  onApply?: () => void;
}) {
  const [state, setState] = useState(initial);
  const change = (field: keyof CodexSubagentSettings, value: SettingValue) => {
    setState((current) => ({
      ...current,
      phase: "dirty",
      draft: { ...current.draft!, [field]: value },
      preview: undefined,
      previewError: undefined,
    }));
  };
  return (
    <CodexSubagentSettingsPanel
      editorState={state}
      busy={false}
      onChange={change}
      onReset={() => setState((current) => ({
        ...current,
        phase: "dirty",
        draft: automatic,
        preview: undefined,
      }))}
      onRetryLoad={vi.fn()}
      onPreview={() => setState((current) => ({ ...current, preview }))}
      onApplyPreview={async () => {
        onApply();
        setState((current) => ({
          ...current,
          phase: "clean",
          snapshot: { ...current.snapshot!, settings: current.draft! },
          preview: undefined,
        }));
        return true;
      }}
    />
  );
}

describe("CodexSubagentSettingsPanel", () => {
  it("uses an explicit preview and confirmation before applying a global runtime setting", async () => {
    const user = userEvent.setup();
    const onApply = vi.fn();
    render(<Harness onApply={onApply} />);

    const enabled = screen.getByRole("radiogroup", { name: "启用子 agent" });
    await user.click(within(enabled).getByRole("radio", { name: "开启" }));
    await user.click(screen.getByRole("button", { name: "生成写入预览" }));

    expect(screen.getByLabelText("Codex 用户级配置 配置预览")).toHaveTextContent("enabled = true");
    await user.click(screen.getByRole("button", { name: "应用子 agent 设置" }));
    const dialog = screen.getByRole("dialog", { name: "确认应用子 agent 设置" });
    expect(within(dialog).getByText("只会变更本模块拥有的三个全局子 agent 键；供应商参数、角色表与其他用户配置保持不变。")).toBeVisible();
    await user.click(within(dialog).getByRole("button", { name: "确认应用" }));

    await waitFor(() => expect(onApply).toHaveBeenCalledOnce());
    expect(screen.queryByRole("dialog", { name: "确认应用子 agent 设置" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "应用子 agent 设置" })).not.toBeInTheDocument();
  });

  it("blocks preview while a custom concurrency value is invalid", async () => {
    const user = userEvent.setup();
    render(<Harness />);

    const concurrency = screen.getByRole("radiogroup", { name: "最大并发子 agent 线程数配置方式" });
    await user.click(within(concurrency).getByRole("radio", { name: "指定" }));
    await user.type(screen.getByRole("textbox", { name: "最大并发子 agent 线程数值" }), "0");

    expect(screen.getByRole("alert")).toHaveTextContent("最大并发必须是可准确表示且不小于 1 的整数");
    expect(screen.getByRole("button", { name: "生成写入预览" })).toBeDisabled();
  });

  it("surfaces a deprecated host key without mapping it into the canonical form", () => {
    render(<Harness initial={editorState(automatic, ["agents.max_threads"])} />);

    expect(screen.getByRole("alert")).toHaveTextContent("agents.max_threads");
    expect(screen.getByRole("alert")).toHaveTextContent("不会读取、转换或写入");
  });
});
