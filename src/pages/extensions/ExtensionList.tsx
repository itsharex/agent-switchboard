import type { ExtensionListItem, SkillUpdateReport } from "../../api/client";
import { Button } from "../../components/Button";
import { PreviewIcon, TrashIcon, UpdateIcon } from "../../components/icons";
import { Table, type TableColumn } from "../../components/Table";
import { Tooltip } from "../../components/Tooltip";
import { ClientToggleGroup } from "../../components/extensions/ClientToggleGroup";
import { MANAGEMENT_CLIENTS } from "../../components/extensions/client-presentation";
import { TRANSPORT_LABELS } from "../../components/extensions/labels";
import { pendingDeployment } from "./pending-deployment";
import type { ExtensionWorkspace } from "./useExtensionWorkspace";

function rowDescription(item: ExtensionListItem) {
  if (item.kind === "skill") return item.manifest.description ?? "";
  if (item.transport === "stdio") return `${item.command} · ${item.argumentCount} 个启动参数`;
  return item.lastCheck
    ? `最近连接检测：${item.lastCheck.outcome.kind === "passed" ? "通过" : "查看检测结果"}`
    : "尚未检测连接";
}

function RowActions({
  item,
  report,
  workspace,
}: {
  item: ExtensionListItem;
  report?: SkillUpdateReport;
  workspace: ExtensionWorkspace;
}) {
  return (
    <div className={`asb-ext-table-actions${report ? " has-update" : ""}`}>
      {report && (
        <Tooltip label={`更新 ${item.name}`}>
          <Button
            variant="icon"
            className="asb-ext-table-action"
            disabled={workspace.writeBlocked}
            aria-label={`更新 ${item.name}`}
            onClick={() => void workspace.updates.update([report])}
          >
            <UpdateIcon />
          </Button>
        </Tooltip>
      )}
      <Tooltip label={`部署与诊断 ${item.name}`}>
        <Button
          variant="icon"
          className="asb-ext-table-action"
          disabled={workspace.writeBlocked}
          aria-label={`打开 ${item.name} 的部署与诊断`}
          onClick={() => workspace.nav.openManagement(item.id, item.kind)}
        >
          <PreviewIcon />
        </Button>
      </Tooltip>
      <Tooltip label={`删除 ${item.name}`}>
        <Button
          variant="icon"
          className="asb-ext-table-action asb-ext-table-action-danger"
          disabled={workspace.writeBlocked}
          aria-label={`删除 ${item.name}`}
          onClick={() => workspace.nav.setDialog({ type: "remove", item })}
        >
          <TrashIcon />
        </Button>
      </Tooltip>
    </div>
  );
}

function ExtensionIdentity({ item, workspace }: { item: ExtensionListItem; workspace: ExtensionWorkspace }) {
  const report = workspace.updates.reportMap.get(item.id);
  const updatable = workspace.updates.updatable.find((entry) => entry.definitionId === item.id);
  const description = rowDescription(item);
  const meta = item.kind === "mcp" ? TRANSPORT_LABELS[item.transport] : item.source?.resolvedCommit ? "GitHub" : "本地";
  return (
    <Button
      variant="unstyled"
      className="asb-ext-table-definition"
      aria-label={item.kind === "mcp" ? `编辑 MCP ${item.name}` : `编辑 Skill ${item.name}`}
      onClick={() => void workspace.edit(item)}
    >
      <span className="asb-ext-table-title">
        <span className="asb-ext-table-name">{item.name}</span>
        <span className="asb-ext-table-meta">{meta}</span>
        {updatable && <span className="asb-ext-update-badge">可更新</span>}
        {report?.error && <span className="asb-ext-table-error" title={report.error}>检查失败</span>}
      </span>
      {description && <span className="asb-ext-table-description" title={description}>{description}</span>}
    </Button>
  );
}

function columns(workspace: ExtensionWorkspace): Array<TableColumn<ExtensionListItem>> {
  return [
    {
      key: "definition",
      header: "扩展",
      cellClassName: "asb-ext-table-definition-cell",
      render: (item) => <ExtensionIdentity item={item} workspace={workspace} />,
    },
    {
      key: "deployments",
      header: "客户端",
      cellClassName: "asb-ext-table-deployments",
      render: (item) => (
        <ClientToggleGroup
          item={item}
          busy={workspace.writeBlocked}
          pendingClients={MANAGEMENT_CLIENTS.filter((client) =>
            pendingDeployment(workspace.applies.pendingOperations, item, client))}
          onToggle={(client) => void workspace.toggleClient(item, client)}
        />
      ),
    },
    {
      key: "actions",
      header: "操作",
      cellClassName: "asb-ext-table-action-cell",
      render: (item) => (
        <RowActions
          item={item}
          report={workspace.updates.updatable.find((entry) => entry.definitionId === item.id)}
          workspace={workspace}
        />
      ),
    },
  ];
}

export function ExtensionList({ workspace }: { workspace: ExtensionWorkspace }) {
  return (
    <Table
      className="asb-ext-table"
      columns={columns(workspace)}
      rows={workspace.visible}
      rowKey={(item) => item.id}
      ariaLabel="扩展列表"
    />
  );
}
