import type { ObservedExtension } from "../../api/client";
import { discoveryImportMode } from "../../app/extensions/useDiscoveryImport";
import { clientName } from "../../lib/client-name";
import { Button } from "../Button";
import { Checkbox } from "../Checkbox";
import { ClientLogo } from "../ClientLogo";
import { Eye } from "lucide-react";

export function discoveryOrigin(item: ObservedExtension, projects: ReadonlyMap<string, string>) {
  if (item.origin.origin === "projectRoot") return "项目 " + (projects.get(item.origin.projectId) ?? "已登记项目");
  if (item.origin.origin === "legacyRoot") return "历史目录（只读）";
  if (item.origin.origin === "managed") return "托管安装（只读）";
  return clientName(item.client) + " 用户级目录";
}

interface Props {
  rows: ObservedExtension[];
  selected: ReadonlySet<string>;
  busy: boolean;
  projects: ReadonlyMap<string, string>;
  warnings: ReadonlyMap<string, number>;
  onSelect: (id: string, checked: boolean) => void;
  onWarning: (id: string) => void;
  onViewDetails: (item: ObservedExtension) => void;
}

function ImportRow({ item, props }: { item: ObservedExtension; props: Props }) {
  const mode = discoveryImportMode(item);
  const warningCount = props.warnings.get(item.observationId) ?? 0;
  const reason = !mode && !item.managed && !item.actions.import.inLibrary
    ? item.actions.takeover.reason ?? item.actions.import.reason : null;
  return (
    <li className="asb-ext-import-row">
      <div className="asb-ext-import-identity">
        <Checkbox label={item.name} ariaLabel={"选择 " + item.name + " 的 " + clientName(item.client) + " 安装"}
          checked={props.selected.has(item.observationId)} disabled={props.busy || mode === null}
          onChange={(checked) => props.onSelect(item.observationId, checked)} />
        {item.description && <p className="asb-ext-import-description">{item.description}</p>}
        <p className="asb-ext-import-origin">
          <ClientLogo app={item.client} className="asb-ext-clienttoggle-logo" />
          {discoveryOrigin(item, props.projects)}
          {item.transport && <span>{item.transport}</span>}
        </p>
        {reason && <p className="asb-warn-text asb-ext-import-description">{reason}</p>}
      </div>
      <div className="asb-ext-import-status">
        {item.managed ? <span>已管理</span> : mode === "copy" ? <span>仅复制</span>
          : mode === "manage" ? <span>保留现有安装</span> : item.actions.import.inLibrary ? <span>已在扩展库</span> : null}
        {warningCount > 0 && <Button variant="unstyled" className="asb-warn-text"
          aria-label={"查看 " + item.name + " 的 " + warningCount + " 条警告"}
          onClick={() => props.onWarning(item.observationId)}>{warningCount} 条警告</Button>}
        {item.actions.managedDefinitionId && <Button variant="icon" className="asb-ext-import-details"
          aria-label={"查看 " + item.name + " 的管理详情"} onClick={() => props.onViewDetails(item)}><Eye /></Button>}
      </div>
    </li>
  );
}

export function DiscoveryImportList(props: Props) {
  return <ul className="asb-ext-import-list" aria-label="本机发现的扩展">
    {props.rows.map((item) => <ImportRow key={item.observationId} item={item} props={props} />)}
  </ul>;
}
