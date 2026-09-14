import { useEffect, useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import * as api from "../../api/claude-accounts";
import type { ProviderModel } from "../../api/providers";
import { Button } from "../Button";
import { Checkbox } from "../Checkbox";
import { Input } from "../Input";
import { Select } from "../Select";
import { Table, type TableColumn } from "../Table";
import { AccountImport } from "./AccountImport";
import { useClaudeLogin } from "./useClaudeLogin";
import type { ClaudeOperations } from "./operations";
const SERVICES = [{ value: "github_copilot", label: "GitHub Copilot" }, { value: "codex_oauth", label: "ChatGPT OAuth（供 Claude 上游使用）" }, { value: "xai_oauth", label: "xAI OAuth" }];
function Quota({ quota }: { quota: api.ClaudeAccountQuota }) {
  return <section aria-label="Claude 托管账号订阅额度"><p>{quota.plan ?? "订阅类型未知"} · {new Date(quota.checkedAtMs).toLocaleString()}</p>
    <Table ariaLabel="Claude 订阅窗口" rows={quota.windows} rowKey={(w) => w.id}
      columns={[{ key: "name", header: "窗口", render: (w) => w.label }, { key: "used", header: "已用", render: (w) => w.unlimited ? "不限额" : w.usedPercent === null ? "未知" : w.usedPercent + "%" },
        { key: "remaining", header: "剩余", render: (w) => w.remaining ?? "未知" }, { key: "reset", header: "重置时间", render: (w) => w.resetsAtMs === null ? "未知" : new Date(w.resetsAtMs).toLocaleString() }]} />
  </section>;
}
export function AccountsPane({ operations: op }: { operations: ClaudeOperations }) {
  const [view, setView] = useState<api.ClaudeAccountsView | null>(null);
  const [provider, setProvider] = useState<api.ClaudeAuthProvider>("github_copilot");
  const [label, setLabel] = useState("");
  const [domain, setDomain] = useState("");
  const [makeDefault, setMakeDefault] = useState(true);
  const [deleting, setDeleting] = useState<api.ClaudeAccountView | null>(null);
  const [detail, setDetail] = useState<{ label: string; quota?: api.ClaudeAccountQuota; models?: ProviderModel[] } | null>(null);
  const login = useClaudeLogin(op, setView);
  const { run, busy, changed } = op;
  useEffect(() => { void run(async () => setView(await api.getClaudeAccounts())); }, [run]);
  const inspect = (account: api.ClaudeAccountView, kind: "models" | "quota") => void run(async () => {
    const value = kind === "models" ? { models: await api.getClaudeAccountModels(account.provider, account.id) } : { quota: await api.getClaudeAccountQuota(account.provider, account.id) };
    setDetail({ label: account.label, ...value }); setView(await api.getClaudeAccounts());
  });
  const columns: TableColumn<api.ClaudeAccountView>[] = [
    { key: "name", header: "账号", render: (a) => a.label + (a.isDefault ? " · 默认" : "") },
    { key: "service", header: "服务", render: (a) => SERVICES.find((s) => s.value === a.provider)?.label ?? a.provider },
    { key: "actions", header: "操作", render: (a) => <div className="asb-form-actions">
      <Button variant="secondary" disabled={busy || a.isDefault} onClick={() => void run(async () => { setView(await api.setClaudeDefaultAccount(a.provider, a.id, view!.fileHash, true)); changed("Claude 默认账号已更新；显式绑定的档案不会改用其它账号。"); })}>设为默认</Button>
      <Button variant="secondary" disabled={busy || !!login.login} onClick={() => login.start({ provider: a.provider, label: a.label, githubDomain: a.githubDomain, targetAccountId: a.id, makeDefault: a.isDefault })}>重新认证</Button>
      <Button variant="secondary" disabled={busy} onClick={() => inspect(a, "models")}>模型</Button><Button variant="secondary" disabled={busy} onClick={() => inspect(a, "quota")}>订阅额度</Button>
      <Button variant="danger" disabled={busy} onClick={() => setDeleting(a)}>删除账号</Button>
    </div> },
  ];
  return <div className="asb-provider-section-fields">
    <p className="asb-scope-note">账号仅供 Claude 上游使用，与原生 Claude 登录及 Codex 账号库分开。账号删除后，显式绑定的档案会报错，不会静默回退。</p>
    <Select ariaLabel="Claude 托管服务" value={provider} disabled={busy || !!login.login} options={SERVICES} onChange={(v) => { setProvider(v as api.ClaudeAuthProvider); setDetail(null); }} />
    <label className="asb-field"><span>新账号名称</span><Input value={label} disabled={busy || !!login.login} onChange={(e) => setLabel(e.target.value)} /></label>
    {provider === "github_copilot" && <label className="asb-field"><span>设备登录 GitHub 域名（可选）</span><Input value={domain} disabled={busy || !!login.login} placeholder="github.com" onChange={(e) => setDomain(e.target.value)} /></label>}
    <Checkbox label="将新登录设为此服务默认账号" checked={makeDefault} disabled={busy || !!login.login} onChange={setMakeDefault} />
    <Button variant="primary" disabled={busy || !!login.login || !label.trim()} onClick={() => login.start({ provider, label: label.trim(), githubDomain: provider === "github_copilot" ? domain.trim() || null : null, targetAccountId: null, makeDefault })}>开始 Claude 托管账号登录</Button>
    {login.login && <section aria-label="Claude 设备授权" className="asb-provider-section-fields">
      <p>授权码：<strong>{login.login.userCode}</strong></p><p className="asb-scope-note">{login.login.verificationUrl} · 有效至 {new Date(login.login.expiresAtMs).toLocaleString()}</p>
      <div className="asb-form-actions"><Button variant="secondary" onClick={() => void run(() => openUrl(login.login!.verificationUrl))}>打开授权页面</Button>
        <Button variant="secondary" onClick={() => void login.cancel()}>取消 Claude 登录</Button></div>
    </section>}
    {view && <><Table ariaLabel="Claude 托管账号" rows={view.accounts} rowKey={(a) => a.id} columns={columns} />
      <AccountImport key={provider} provider={provider} view={view} operations={op} onSaved={setView} /></>}
    {deleting && view && <section aria-label="确认删除 Claude 账号"><p>删除 {deleting.label}？只删除本地托管凭据，不注销远程账户。</p>
      <Button variant="secondary" disabled={busy} onClick={() => setDeleting(null)}>取消删除账号</Button>
      <Button variant="danger" disabled={busy} onClick={() => void run(async () => { setView(await api.removeClaudeAccount(deleting.id, view.fileHash, true)); setDeleting(null); setDetail(null); changed("Claude 本地账号已删除。"); })}>确认删除 Claude 账号</Button></section>}
    {detail && <section aria-label="Claude 账号查询结果"><h3 className="asb-section-title">{detail.label}</h3>{detail.quota && <Quota quota={detail.quota} />}
      {detail.models && <Table ariaLabel="Claude 账号模型" rows={detail.models} rowKey={(m) => m.id} columns={[{ key: "id", header: "模型 ID", render: (m) => m.id }]} />}</section>}
    <Button variant="secondary" disabled={busy} onClick={() => void run(async () => setView(await api.getClaudeAccounts()))}>重新读取 Claude 账号</Button>
  </div>;
}
