import { useState } from "react";
import * as api from "../../api/claude-accounts";
import { Button } from "../Button";
import { Checkbox } from "../Checkbox";
import { Input } from "../Input";
import type { ClaudeOperations } from "./operations";
export function AccountImport({ provider, view, operations: op, onSaved }: {
  provider: api.ClaudeAuthProvider; view: api.ClaudeAccountsView; operations: ClaudeOperations; onSaved: (view: api.ClaudeAccountsView) => void;
}) {
  const [open, setOpen] = useState(false);
  const [label, setLabel] = useState("");
  const [access, setAccess] = useState("");
  const [refresh, setRefresh] = useState("");
  const [workspace, setWorkspace] = useState("");
  const [expiry, setExpiry] = useState("");
  const [domain, setDomain] = useState("");
  const [makeDefault, setMakeDefault] = useState(true);
  const copilot = provider === "github_copilot";
  const valid = label.trim() && access.trim() && (copilot || Number.isFinite(Date.parse(expiry))) && (provider !== "codex_oauth" || workspace.trim());
  if (!open) return <Button variant="secondary" disabled={op.busy} onClick={() => setOpen(true)}>手动导入 Claude 托管凭据</Button>;
  return <section aria-label="手动导入 Claude 托管凭据" className="asb-provider-section-fields">
    <p className="asb-scope-note">只导入你明确提供的凭据，不读取 Claude 或 Codex 的原生登录缓存。Copilot 请填 GitHub 登录令牌，而非短期 Copilot 服务令牌。</p>
    <label className="asb-field"><span>导入账号名称</span><Input value={label} disabled={op.busy} onChange={(e) => setLabel(e.target.value)} /></label>
    <label className="asb-field"><span>访问令牌</span><Input type="password" value={access} disabled={op.busy} onChange={(e) => setAccess(e.target.value)} /></label>
    {copilot ? <label className="asb-field"><span>GitHub 域名（可选）</span><Input value={domain} disabled={op.busy} placeholder="github.com" onChange={(e) => setDomain(e.target.value)} /></label> : <>
      <label className="asb-field"><span>刷新令牌（可选）</span><Input type="password" value={refresh} disabled={op.busy} onChange={(e) => setRefresh(e.target.value)} /></label>
      <label className="asb-field"><span>访问令牌到期时间</span><Input type="datetime-local" value={expiry} disabled={op.busy} onChange={(e) => setExpiry(e.target.value)} /></label>
      <label className="asb-field"><span>上游 workspace/account ID{provider === "codex_oauth" ? "（必填）" : "（可选）"}</span><Input value={workspace} disabled={op.busy} onChange={(e) => setWorkspace(e.target.value)} /></label>
    </>}
    <Checkbox label="将导入账号设为此服务默认账号" checked={makeDefault} disabled={op.busy} onChange={setMakeDefault} />
    <div className="asb-form-actions"><Button variant="secondary" disabled={op.busy} onClick={() => { setOpen(false); setAccess(""); setRefresh(""); }}>取消导入</Button>
      <Button variant="primary" disabled={op.busy || !valid} onClick={() => void op.run(async () => {
        const next = await api.saveClaudeAccount({ id: crypto.randomUUID(), label: label.trim(), provider, accessToken: access.trim(), refreshToken: copilot ? null : refresh.trim() || null,
          expiresAtMs: copilot ? null : Date.parse(expiry), upstreamAccountId: copilot ? null : workspace.trim() || null, githubDomain: copilot ? domain.trim() || null : null }, view.fileHash, makeDefault, true);
        setAccess(""); setRefresh(""); setOpen(false); onSaved(next); op.changed("Claude 托管凭据已导入独立账号库。");
      })}>确认导入账号</Button></div>
  </section>;
}
