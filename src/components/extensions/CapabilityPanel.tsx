import type { ClientCapabilityReport } from "../../api/client";
import { clientName } from "../../lib/client-name";
import { DashIcon } from "../icons";

interface Props {
  reports: ClientCapabilityReport[];
}

/** Read-only capability audit for both clients. It is secondary diagnostic
 * detail, so it stays collapsed until the user asks to inspect it. */
export function CapabilityPanel({ reports }: Props) {
  return (
    <details className="asb-ext-disclosure asb-ext-capabilities" aria-label="客户端能力">
      <summary>
        <span className="asb-section-title">客户端能力</span>
        <span className="asb-scope-note">{reports.length} 个客户端</span>
      </summary>
      <div className="asb-ext-capability-grid">
        {reports.map((report) => (
          <section key={report.client} aria-label={`${clientName(report.client)} 能力`}>
            <h4 className="asb-group-title">{clientName(report.client)}</h4>
            {report.entries.length === 0 ? (
              <div className="asb-empty-state">
                <span className="asb-empty-state-icon" aria-hidden="true">
                  <DashIcon />
                </span>
                <h3 className="asb-section-title">无能力记录</h3>
              </div>
            ) : (
              <ul className="asb-ext-capability-list">
                {report.entries.map((entry) =>
                  entry.verification === "verified" ? (
                    <li key={entry.code}>
                      <span>{entry.resource}</span>{" "}
                      {entry.supported ? (
                        <span className="asb-ok-text">已核实 · {entry.clientVersion}</span>
                      ) : (
                        <span className="asb-warn-text">未核实</span>
                      )}
                    </li>
                  ) : (
                    <li key={entry.code}>
                      <span>{entry.resource}</span>{" "}
                      <span className="asb-warn-text">未核实 · {entry.condition}</span>
                    </li>
                  ),
                )}
              </ul>
            )}
          </section>
        ))}
      </div>
    </details>
  );
}
