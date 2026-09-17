import { useEffect, useState } from "react";
import * as api from "../../api/claude-integration";
import { Button } from "../Button";
import { Checkbox } from "../Checkbox";
import { DiffView } from "../DiffView";
import { ClientManagementModule } from "../client-management/ClientManagementModule";
import type { ClaudeOperations } from "./operations";

const FLAGS: Record<api.ClaudeIntegrationFlag, { title: string; note: string }> = {
  plugin: { title: "插件 API Key 标记", note: "第三方供应商生效时写入 primaryApiKey=any，Claude Code 插件 / VS Code 扩展不再要求登录；官方登录时清除。" },
  onboarding: { title: "跳过首次引导", note: "写入 hasCompletedOnboarding=true，新环境启动 Claude Code 时不再进入首次引导。" },
};

/** Both markers live in Claude's own client files; every write is previewed,
 * confirmed, locked, backed up, and atomically replaced. */
export function IntegrationPane({ operations: op }: { operations: ClaudeOperations }) {
  const [view, setView] = useState<api.ClaudeIntegrationView | null>(null);
  const [preview, setPreview] = useState<api.ClaudeIntegrationPreview | null>(null);
  const { run, busy, changed } = op;
  const reload = () => void run(async () => setView(await api.getClaudeIntegration()));
  const plan = (flag: api.ClaudeIntegrationFlag, enable: boolean) => void run(async () => {
    setPreview(await api.previewClaudeIntegration(flag, enable));
  });

  useEffect(() => { void run(async () => setView(await api.getClaudeIntegration())); }, [run]);

  return (
    <ClientManagementModule
      title="客户端集成"
      description="仅改写 Claude 客户端文件中的单个键，其余内容原样保留；备份与配置切换备份分开保存。"
      refreshLabel="重新读取 Claude 客户端集成状态"
      busy={busy}
      refreshDisabled={busy || !!preview}
      onRefresh={reload}
    >
      {view ? <>
        <Checkbox label="切换 Claude 供应商后自动同步插件 API Key 标记" checked={view.policy.pluginIntegration} disabled={busy || !!preview}
          onChange={(pluginIntegration) => void run(async () => {
            setView(await api.setClaudeIntegrationPolicy({ pluginIntegration }, true));
            changed("Claude 客户端集成策略已保存。");
          })} />
        {view.flags.map((flag) => <section key={flag.flag} aria-label={FLAGS[flag.flag].title} className="asb-client-management-group">
          <h4 className="asb-group-title">{FLAGS[flag.flag].title}</h4>
          <p className="asb-scope-note">{FLAGS[flag.flag].note}</p>
          <p>{flag.exists ? (flag.applied ? "已写入" : "未写入") : "文件不存在（写入时创建）"} · {flag.target}</p>
          <div className="asb-form-actions">
            <Button variant="secondary" disabled={busy || !!preview || flag.applied} onClick={() => plan(flag.flag, true)}>预览写入{FLAGS[flag.flag].title}</Button>
            <Button variant="secondary" disabled={busy || !!preview || !flag.applied} onClick={() => plan(flag.flag, false)}>预览清除{FLAGS[flag.flag].title}</Button>
          </div>
        </section>)}
      </> : <p role="status">正在读取 Claude 客户端集成状态…</p>}
      {preview && <section aria-label="Claude 客户端集成预览" className="asb-client-management-group">
        <h4 className="asb-group-title">变更预览</h4>
        <p className="asb-scope-note">{preview.target}</p>
        {preview.changes.length === 0 ? <p role="status">已是目标状态，无需写入。</p> : <DiffView label="Claude 客户端集成变更" changes={preview.changes} />}
        <div className="asb-form-actions">
          <Button variant="secondary" disabled={busy} onClick={() => setPreview(null)}>取消集成预览</Button>
          {preview.changes.length > 0 && <Button variant="primary" disabled={busy} onClick={() => void run(async () => {
            setView(await api.applyClaudeIntegration(preview, true));
            setPreview(null);
            changed("Claude 客户端集成标记已写入并备份。");
          })}>确认写入 Claude 客户端标记</Button>}
        </div>
      </section>}
    </ClientManagementModule>
  );
}