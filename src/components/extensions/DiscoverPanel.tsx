import { useEffect, useRef } from "react";
import type { ExtensionDiscoveryDiagnostic, ObservedExtension } from "../../api/client";
import { Button } from "../Button";
import { clientName } from "../../lib/client-name";

interface Props {
  observations: ObservedExtension[] | null;
  diagnostics: ExtensionDiscoveryDiagnostic[];
  loading: boolean;
  busy: boolean;
  onScan: () => void;
  onImportSkill: (observed: ObservedExtension) => void;
  onImportMcp: (observed: ObservedExtension) => void;
  /** Opens the takeover preview for an unmanaged native item. */
  onTakeover: (observed: ObservedExtension) => void;
}

function originLabel(observed: ObservedExtension): string {
  switch (observed.origin.origin) {
    case "userRoot":
      return `${clientName(observed.client)} 用户级目录`;
    case "legacyRoot":
      return "历史目录（只读）";
    case "projectRoot":
      return "已登记项目目录";
    case "managed":
      return "托管安装（只读）";
  }
}

/** Read-only list of Skills and MCP servers already present on this machine;
 * entries can be imported into the library but are never modified here. */
export function DiscoverPanel({
  observations,
  diagnostics,
  loading,
  busy,
  onScan,
  onImportSkill,
  onImportMcp,
  onTakeover,
}: Props) {
  const startedRef = useRef(false);

  useEffect(() => {
    if (startedRef.current || observations !== null) return;
    startedRef.current = true;
    onScan();
  }, [observations, onScan]);

  return (
    <div className="asb-ext-section" aria-label="从本机发现">
      <div className="asb-ext-actions">
        <Button variant="secondary" disabled={busy} onClick={onScan}>
          {observations === null ? "扫描本机" : "重新扫描"}
        </Button>
        {loading && <span className="asb-scope-note">正在扫描…</span>}
      </div>
      {diagnostics.length > 0 && (
        <div className="asb-banner asb-banner-warning" role="status" aria-label="发现诊断">
          {diagnostics.map((diagnostic) => (
            <p key={`${diagnostic.client}-${diagnostic.message}`}>
              {clientName(diagnostic.client)}：{diagnostic.message}
            </p>
          ))}
        </div>
      )}
      {observations !== null &&
        (observations.length === 0 ? (
          <p className="asb-empty">未在本机发现已有的 Skill 或 MCP 服务</p>
        ) : (
          <ul className="asb-ext-discover-list">
            {observations.map((observed) => (
              <li key={observed.observationId} className="asb-ext-discover-item">
                <div className="asb-ext-history-head">
                  <strong>{observed.name}</strong>
                  <span className="asb-pill-status">{observed.kind === "skill" ? "Skill" : "MCP"}</span>
                  {observed.transport && <span className="asb-scope-note">{observed.transport}</span>}
                  <span className="asb-scope-note">{originLabel(observed)}</span>
                  {observed.managed && (
                    <span className="asb-pill-status">已由本应用管理</span>
                  )}
                  {observed.kind === "mcp" ? (
                    <>
                      <Button
                        variant="secondary"
                        disabled={busy}
                        onClick={() => onImportMcp(observed)}
                      >
                        导入到库
                      </Button>
                      {!observed.managed && (
                        <Button
                          variant="secondary"
                          disabled={busy}
                          onClick={() => onTakeover(observed)}
                        >
                          接管
                        </Button>
                      )}
                    </>
                  ) : (
                    <>
                      <Button
                        variant="secondary"
                        disabled={busy || !observed.contentDigest}
                        onClick={() => onImportSkill(observed)}
                      >
                        导入
                      </Button>
                      {!observed.managed && observed.contentDigest && (
                        <Button
                          variant="secondary"
                          disabled={busy}
                          onClick={() => onTakeover(observed)}
                        >
                          接管
                        </Button>
                      )}
                    </>
                  )}
                </div>
                {observed.kind === "skill" && observed.contentDigest && (
                  <p className="asb-discovery-basis">
                    内容摘要 <span className="asb-code">{observed.contentDigest.slice(0, 12)}</span>
                  </p>
                )}
                {observed.diagnostics.map((diagnostic) => (
                  <p key={diagnostic} className="asb-warn-text">
                    {diagnostic}
                  </p>
                ))}
              </li>
            ))}
          </ul>
        ))}
    </div>
  );
}
