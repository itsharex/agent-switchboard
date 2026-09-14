import { useEffect, useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import * as api from "../../api/codex-accounts";
import { listProfiles, type ProviderRecord } from "../../api/providers";
import { Button } from "../Button";
import { Checkbox } from "../Checkbox";
import { Select } from "../Select";
import { Table, type TableColumn } from "../Table";
import { QuotaWindowsTable } from "../QuotaWindowsTable";
import { useManagedLogin } from "./useManagedLogin";
import { SwitchConfirmation } from "./SwitchConfirmation";
import type { CodexOperations } from "./operations";
export function AccountsPane({ operations: op }: { operations: CodexOperations }) {
  const [policy, setPolicy] = useState<api.CodexAuthPolicyView | null>(null);
  const [view, setView] = useState<api.CodexAccountsView | null>(null);
  const [profiles, setProfiles] = useState<ProviderRecord[]>([]);
  const [profileId, setProfileId] = useState<string | null>(null);
  const [apply, setApply] = useState(false);
  const [detail, setDetail] = useState<api.CodexAccountModels | api.CodexAccountQuota | null>(null);
  const [deleting, setDeleting] = useState<string | null>(null);
  const login = useManagedLogin(op, setView);
  const { run, busy, changed } = op;
  useEffect(() => { void run(async () => {
    const [accounts, records, authPolicy] = await Promise.all([api.listCodexAccounts(), listProfiles(), api.getCodexAuthPolicy()]);
    setPolicy(authPolicy);
    const official = records.filter((p) => p.profile.app === "codex" && p.profile.routeMode === "official");
    setView(accounts); setProfiles(official); setProfileId(official[0]?.profile.id ?? null);
  }); }, [run]);
  if (!view) return <p role="status">正在读取 Codex 账号…</p>;
  const binding = profileId ? view.bindings[profileId] : undefined;
  const value = binding?.kind === "account" ? binding.id : binding?.kind ?? "native";
  const columns: TableColumn<api.CodexAccountSummary>[] = [
    { key: "name", header: "本地账号", render: (a) => <>{a.email ?? a.accountLabel}{a.isDefault ? " · 默认" : ""}<br />{a.plan}{a.nativeSyncPending && <p className="asb-warn-text">原生令牌同步待恢复</p>}{a.nativeSyncError && <p className="asb-warn-text">{a.nativeSyncError}</p>}</> },
    { key: "actions", header: "操作", render: (a) => <div className="asb-provider-field-grid">
      <Button variant="secondary" disabled={busy || !!login.login} onClick={() => void run(async () => setView(await api.setCodexDefaultAccount(a.id, view.revision)))}>设为默认</Button>
      <Button variant="secondary" disabled={busy || !!login.login} onClick={() => login.start(a.id)}>重新认证</Button>
      <Button variant="secondary" disabled={busy} onClick={() => void run(async () => { setDetail(await api.getCodexAccountModels(a.id)); setView(await api.listCodexAccounts()); })}>模型</Button>
      <Button variant="secondary" disabled={busy} onClick={() => void run(async () => { setDetail(await api.getCodexAccountQuota(a.id)); setView(await api.listCodexAccounts()); })}>额度</Button>
      <Button variant="danger" disabled={busy} onClick={() => setDeleting(a.id)}>删除账号</Button>
    </div> },
  ];
  return <div className="asb-provider-section-fields">
    <p className="asb-scope-note">未绑定时沿用原生登录；显式绑定不会随默认账号改变。绑定保存后仍需预览并确认应用。</p>
    {policy && <Checkbox label="切换第三方时保留官方登录令牌" checked={policy.policy.preserveOfficialLogin} disabled={busy}
      onChange={(preserve) => void run(async () => { setPolicy(await api.setCodexAuthPolicy(preserve, policy.revision)); changed("认证保留策略已保存，将在下次确认切换时生效。"); })} />}
    <div className="asb-form-actions">
      <Button variant="primary" disabled={busy || !!login.login} onClick={() => login.start(null)}>添加 Codex 账号</Button>
      <Button variant="secondary" disabled={busy || !!login.login} onClick={() => void run(async () => { setView(await api.importCodexNativeAccount(view.revision, true)); changed("已导入本地账号；原生登录未改动。"); })}>确认导入当前原生登录</Button>
    </div>
    {login.login && <section aria-label="Codex 账号登录"><p>设备码：<strong>{login.login.userCode}</strong></p>
      <Button variant="secondary" onClick={() => void openUrl(login.login!.verificationUrl)}>打开官方验证页</Button>
      <Button variant="secondary" onClick={() => void run(login.cancel)}>取消账号登录</Button></section>}
    <Table ariaLabel="Codex 托管账号" rows={view.accounts} columns={columns} rowKey={(a) => a.id} />
    {deleting && <div role="group" aria-label="确认删除 Codex 账号"><p>删除本地托管账号不会退出原生登录。已绑定的账号须先解绑。</p>
      <Button variant="secondary" disabled={busy} onClick={() => setDeleting(null)}>取消删除</Button>
      <Button variant="danger" disabled={busy} onClick={() => void run(async () => { setView(await api.deleteCodexAccount(deleting, view.revision, true)); setDeleting(null); setDetail(null); })}>确认删除账号</Button></div>}
    <Select ariaLabel="绑定的 Codex 官方档案" value={profileId} disabled={busy} options={profiles.map((p) => ({ value: p.profile.id, label: p.profile.name }))} onChange={(id) => { setProfileId(id); setApply(false); }} />
    <Select ariaLabel="Codex 账号绑定" value={value} disabled={busy || !profileId} options={[{ value: "native", label: "沿用原生登录" }, { value: "default", label: "默认托管账号" }, ...view.accounts.map((a) => ({ value: a.id, label: a.email ?? a.accountLabel }))]}
      onChange={(next) => void run(async () => { if (profileId) { setView(await api.setCodexAccountBinding(profileId, next === "native" || next === "default" ? { kind: next } : { kind: "account", id: next }, view.revision)); setApply(false); changed("账号绑定已保存，确认应用后才会改变 Codex 登录。"); } })} />
    <Button variant="secondary" disabled={busy || !profileId} onClick={() => setApply(true)}>预览并应用账号绑定</Button>
    {apply && profileId && <SwitchConfirmation profileId={profileId} operations={op} onClose={() => setApply(false)} />}
    {detail?.warning && <p className="asb-warn-text" role="alert">{detail.warning}</p>}
    <Button variant="secondary" disabled={busy} onClick={() => void run(async () => { setView(await api.listCodexAccounts()); setPolicy(await api.getCodexAuthPolicy()); })}>重新读取账号</Button>
    {detail && ("models" in detail ? <ul aria-label="账号可用模型">{detail.models.map((m) => <li key={m.id}>{m.displayName} · {m.contextWindow ?? "窗口未声明"}</li>)}</ul> : <section><p role="status">额度状态：{detail.quota.status}{detail.quota.stale ? "（上次成功缓存）" : ""}</p><QuotaWindowsTable windows={detail.quota.windows} ariaLabel="所选账号订阅额度" /></section>)}
  </div>;
}
