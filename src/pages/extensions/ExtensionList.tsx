import type { ExtensionListItem, SkillUpdateReport } from "../../api/client";
import { Button } from "../../components/Button";
import { EditIcon, TrashIcon, UpdateIcon } from "../../components/icons";
import { Tooltip } from "../../components/Tooltip";
import { ClientToggleGroup } from "../../components/extensions/ClientToggleGroup";
import { TRANSPORT_LABELS } from "../../components/extensions/labels";
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
  const editLabel = item.kind === "mcp" ? `编辑定义 ${item.name}` : `编辑内容 ${item.name}`;
  return (
    <div className="asb-ext-row-actions">
      {report && (
        <Tooltip label={`更新 ${item.name}`}>
          <Button
            variant="icon"
            className="asb-ext-rowbtn"
            disabled={workspace.writeBlocked}
            aria-label={`更新 ${item.name}`}
            onClick={() => void workspace.updates.update([report])}
          >
            <UpdateIcon />
          </Button>
        </Tooltip>
      )}
      <Tooltip label={editLabel}>
        <Button
          variant="icon"
          className="asb-ext-rowbtn"
          disabled={workspace.writeBlocked}
          aria-label={editLabel}
          onClick={() => void workspace.edit(item)}
        >
          <EditIcon />
        </Button>
      </Tooltip>
      <Tooltip label={`删除 ${item.name}`}>
        <Button
          variant="icon"
          className="asb-ext-rowbtn asb-ext-rowbtn-danger"
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

function ExtensionRow({ item, workspace }: { item: ExtensionListItem; workspace: ExtensionWorkspace }) {
  const report = workspace.updates.reportMap.get(item.id);
  const updatable = workspace.updates.updatable.find((entry) => entry.definitionId === item.id);
  const description = rowDescription(item);
  const meta = item.kind === "mcp" ? TRANSPORT_LABELS[item.transport] : item.source ? "来源已关联" : "本地";
  return (
    <li className="asb-ext-row">
      <Button
        variant="unstyled"
        className="asb-ext-row-main"
        aria-label={`管理 ${item.name}`}
        onClick={() => workspace.nav.showDefinition(item.id, item.kind)}
      >
        <span className="asb-ext-row-title">
          <span className="asb-ext-row-name">{item.name}</span>
          <span className="asb-ext-row-meta">{meta}</span>
          {updatable && <span className="asb-ext-update-badge">可更新</span>}
          {report?.error && (
            <span className="asb-ext-row-error" title={report.error}>
              检查失败
            </span>
          )}
        </span>
        {description && (
          <span className="asb-ext-row-desc" title={description}>
            {description}
          </span>
        )}
      </Button>
      <ClientToggleGroup
        item={item}
        busy={workspace.writeBlocked}
        onToggle={(client) => void workspace.toggleClient(item, client)}
      />
      <RowActions item={item} report={updatable} workspace={workspace} />
    </li>
  );
}

export function ExtensionList({ workspace }: { workspace: ExtensionWorkspace }) {
  return (
    <ul className="asb-ext-rows" aria-label="扩展列表">
      {workspace.visible.map((item) => (
        <ExtensionRow key={item.id} item={item} workspace={workspace} />
      ))}
    </ul>
  );
}
