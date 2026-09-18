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

function markerState(flag: api.ClaudeIntegrationFlagState): string {
  return flag.exists ? (flag.applied ? "已写入" : "未写入") : "待创建";
}

/** Both markers live in Claude's own client files; every write is previewed,
 * confirmed, locked, backed up, and atomically replaced. Each marker renders
 * as one status-first ledger row whose preview opens inside the row. */
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
      description="直接改写 Claude 客户端文件中的单个键，其余内容原样保留。"
      refreshLabel="重新读取 Claude 客户端集成状态"
      busy={busy}
      refreshDisabled={busy || !!preview}
      onRefresh={reload}
    >
      {view ? view.flags.map((flag) => {
        const meta = FLAGS[flag.flag];
        const rowPreview = preview?.flag === flag.flag ? preview : null;
        return (
          <section key={flag.flag} aria-label={meta.title} className="asb-client-management-group">
            <div className="asb-client-management-marker-head">
              <span className={`asb-status-pill${flag.applied ? " is-ok" : " is-idle"}`}>
                <span className="asb-status-pill-dot" aria-hidden="true" />{markerState(flag)}
              </span>
              <h4 className="asb-group-title">{meta.title}</h4>
            </div>
            <p className="asb-scope-note">{meta.note}</p>
            <p className="asb-client-management-marker-meta">
              <code>{flag.target}</code>{flag.exists ? "" : "（写入时创建）"}
            </p>
            {flag.flag === "plugin" && (
              <Checkbox
                label="切换供应商后自动同步此标记"
                checked={view.policy.pluginIntegration}
                disabled={busy || !!preview}
                onChange={(pluginIntegration) => void run(async () => {
                  setView(await api.setClaudeIntegrationPolicy({ pluginIntegration }, true));
                  changed("Claude 客户端集成策略已保存。");
                })} />
            )}
            <div className="asb-form-actions">
              <Button variant="secondary" disabled={busy || !!preview || flag.applied} onClick={() => plan(flag.flag, true)}>写入</Button>
              <Button variant="secondary" disabled={busy || !!preview || !flag.applied} onClick={() => plan(flag.flag, false)}>清除</Button>
            </div>
            {rowPreview && (
              <section aria-label={`${meta.title}变更预览`} className="asb-client-management-preview">
                <p className="asb-scope-note">写入前备份，与配置切换备份分开保存。</p>
                {rowPreview.changes.length === 0
                  ? <p role="status">已是目标状态，无需写入。</p>
                  : <DiffView label={`${meta.title}变更`} changes={rowPreview.changes} />}
                <div className="asb-form-actions">
                  <Button variant="secondary" disabled={busy} onClick={() => setPreview(null)}>取消</Button>
                  {rowPreview.changes.length > 0 && <Button variant="primary" disabled={busy} onClick={() => void run(async () => {
                    setView(await api.applyClaudeIntegration(rowPreview, true));
                    setPreview(null);
                    changed("Claude 客户端集成标记已写入并备份。");
                  })}>确认写入</Button>}
                </div>
              </section>
            )}
          </section>
        );
      }) : <div className="asb-client-management-skeleton" role="status" aria-label="正在读取">
        <div className="asb-skeleton" />
        <div className="asb-skeleton" />
        <div className="asb-skeleton" />
      </div>}
    </ClientManagementModule>
  );
}
