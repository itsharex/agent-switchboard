import type { AppKind, ClaudeImportProposal, CodexImportProposal, DiscoveredFile, DiscoveryReport } from "../../api/client";
import { Button } from "../../components/Button";
import { ModuleHeader } from "../../components/WorkspaceHeader";
import { SearchIcon } from "../../components/icons";
import { clientName } from "../../lib/client-name";

interface LocalConfigImportProps {
  app: AppKind;
  discovery: DiscoveryReport | null;
  busy: boolean;
  onScan: () => void;
  onImport: () => void;
}

function stateLabel(file: DiscoveredFile): string {
  switch (file.state.kind) {
    case "ok": return "配置可读取";
    case "missing": return "未找到配置文件";
    case "readError": return "读取失败";
    case "parseError": return "语法错误";
  }
}

function LocalRouteFacts({ file }: { file: DiscoveredFile }) {
  if (file.state.kind !== "ok") return null;
  const { route, managed, warnings, importable } = file.state;
  return (
    <>
      <div className="asb-status-row"><dt>当前服务</dt><dd>
        {route.routeMode === "official" ? "官方登录" : "自定义服务"} · {route.model ?? "默认模型"}
      </dd></div>
      {route.providerName && <div className="asb-status-row"><dt>供应商</dt><dd>{route.providerName}</dd></div>}
      {route.baseUrl && <div className="asb-status-row"><dt>服务地址</dt><dd className="asb-code">{route.baseUrl}</dd></div>}
      {route.apiKey && <div className="asb-status-row"><dt>凭据变量</dt><dd className="asb-code">{route.apiKey}</dd></div>}
      <div className="asb-status-row"><dt>管理状态</dt><dd>{managed ? "已由本应用管理" : "未由本应用管理"}</dd></div>
      {(warnings.length > 0 || (!importable && !managed)) && (
        <div className="asb-status-row"><dt>警告</dt><dd>
          {warnings.map((warning) => <span key={warning} className="asb-warn-text asb-status-warn">{warning}</span>)}
          {!importable && !managed && <span className="asb-warn-text asb-status-warn">当前配置包含无法安全导入的设置。</span>}
        </dd></div>
      )}
    </>
  );
}

function LocalConfigCard({ file, proposal, busy, onImport }: {
  file: DiscoveredFile;
  proposal: ClaudeImportProposal | CodexImportProposal | undefined;
  busy: boolean;
  onImport: () => void;
}) {
  return (
    <article className="asb-status-card" aria-label={`${clientName(file.app)} 扫描结果`}>
      <header className="asb-status-head">
        <h3 className="asb-status-name">{clientName(file.app)}</h3>
        <span className="asb-status-pill">{stateLabel(file)}</span>
      </header>
      <dl className="asb-status-rows">
        <div className="asb-status-row"><dt>配置文件</dt><dd className="asb-code">{file.path}</dd></div>
        {file.state.kind === "readError" && (
          <div className="asb-status-row"><dt>读取错误</dt><dd className="asb-warn-text">{file.state.message}</dd></div>
        )}
        {file.state.kind === "parseError" && (
          <div className="asb-status-row"><dt>语法错误</dt><dd className="asb-warn-text">
            {file.state.line !== null ? `第 ${file.state.line} 行 · ` : ""}{file.state.message}
          </dd></div>
        )}
        <LocalRouteFacts file={file} />
      </dl>
      {proposal && <div className="asb-discovery-import">
        <p className="asb-discovery-basis">{proposal.basis}</p>
        <Button variant="secondary" disabled={busy} onClick={onImport}>导入供应商</Button>
      </div>}
    </article>
  );
}

export function LocalConfigImport({ app, discovery, busy, onScan, onImport }: LocalConfigImportProps) {
  return (
    <section className="asb-panel" aria-label="从本机配置导入">
      <ModuleHeader
        title="本机配置"
        primaryActions={
          <Button variant="secondary" disabled={busy} onClick={onScan}>{discovery ? "刷新配置" : "扫描配置"}</Button>
        }
      />
      {discovery ? <LocalConfigCard file={discovery[app]} busy={busy} onImport={onImport}
        proposal={app === "claude" ? discovery.claudeImportProposals[0] : discovery.codexImportProposals[0]} />
        : (
          <div className="asb-empty-state">
            <span className="asb-empty-state-icon" aria-hidden="true">
              <SearchIcon />
            </span>
            <h3 className="asb-section-title">尚未扫描 {clientName(app)} 配置。</h3>
          </div>
        )}
    </section>
  );
}
