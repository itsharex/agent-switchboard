import { useEffect, useMemo, useState } from "react";
import type {
  AppKind,
  BindingStatus,
  CommandError,
  ExtensionDiscoveryDiagnostic,
  ExtensionDraft,
  ExtensionKind,
  ExtensionListItem,
  ExtensionPlanView,
  McpEditRequest,
  McpEditViewEnvelope,
  ObservedExtension,
  SkillUpdateReport,
  TakeoverPreview,
} from "../api/client";
import { useExtensions } from "../app/useExtensions";
import { Button } from "../components/Button";
import { Checkbox } from "../components/Checkbox";
import { Input } from "../components/Input";
import { Select } from "../components/Select";
import { Table, type TableColumn } from "../components/Table";
import { DiscoverPanel } from "../components/extensions/DiscoverPanel";
import { ExtensionDetail } from "../components/extensions/ExtensionDetail";
import { ExtensionHistory } from "../components/extensions/ExtensionHistory";
import { McpEditForm } from "../components/extensions/McpEditForm";
import {
  ExtensionPlanSheet,
  ExtensionRemoveSheet,
  SkillDisableScopeSheet,
} from "../components/extensions/ExtensionPlanSheet";
import { NewMcpForm } from "../components/extensions/NewMcpForm";
import { SkillSourceForm } from "../components/extensions/SkillSourceForm";
import { SkillWorkbench } from "../components/extensions/SkillWorkbench";
import { parseTargetValue } from "../components/extensions/labels";
import { toast } from "../components/use-toast";
import { AddExtensionCards } from "./extensions/AddExtensionCards";
import { NewSkillForm } from "./extensions/NewSkillForm";
import { PortableExportSheet } from "./extensions/PortableExportSheet";
import { PortableImportForm } from "./extensions/PortableImportForm";
import { ProjectRegisterForm } from "./extensions/ProjectRegisterForm";
import { SkillBatchUpdatePanel } from "./extensions/SkillBatchUpdatePanel";
import { TakeoverConfirmSheet } from "./extensions/TakeoverConfirmSheet";
import {
  CLIENT_FILTER_OPTIONS,
  clientSummary,
  matchesClient,
  searchNeedle,
  type AddMode,
  type ClientFilter,
} from "./extensions/list-filters";

interface ExtensionsPageProps {
  busy: boolean;
  setBusy: (busy: boolean) => void;
  clearError: () => void;
  onError: (error: CommandError) => void;
}

/** Extensions workspace: the Skills / MCP library, its deployment bindings,
 * plan-confirmed writes, connection checks, and operation history. */
export function ExtensionsPage({ busy, setBusy, clearError, onError }: ExtensionsPageProps) {
  const ext = useExtensions({ busy, setBusy, clearError, onError });
  const workspace = ext.workspace;
  const [kindTab, setKindTab] = useState<ExtensionKind>("skill");
  const [clientFilter, setClientFilter] = useState<ClientFilter>("all");
  const [search, setSearch] = useState("");
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [addMode, setAddMode] = useState<AddMode>(null);
  const [observations, setObservations] = useState<ObservedExtension[] | null>(null);
  const [discoveryDiagnostics, setDiscoveryDiagnostics] = useState<ExtensionDiscoveryDiagnostic[]>([]);
  const [installTargets, setInstallTargets] = useState<string[]>([]);
  const [updateReport, setUpdateReport] = useState<SkillUpdateReport | null>(null);
  const [skillSelection, setSkillSelection] = useState<ReadonlySet<string>>(new Set());
  const [batchReports, setBatchReports] = useState<SkillUpdateReport[] | null>(null);
  const [batchChecked, setBatchChecked] = useState<ReadonlySet<string>>(new Set());
  const [editView, setEditView] = useState<McpEditViewEnvelope | null>(null);
  const [skillWorkbenchId, setSkillWorkbenchId] = useState<string | null>(null);
  const [newSkillName, setNewSkillName] = useState("");
  const [newSkillDescription, setNewSkillDescription] = useState("");
  const [planView, setPlanView] = useState<ExtensionPlanView | null>(null);
  const [removeTarget, setRemoveTarget] = useState<ExtensionListItem | null>(null);
  const [skillDisableBinding, setSkillDisableBinding] = useState<BindingStatus | null>(null);
  const [skillDisableSharedSettings, setSkillDisableSharedSettings] = useState<boolean | null>(null);
  const [projectFormOpen, setProjectFormOpen] = useState(false);
  const [projectRoot, setProjectRoot] = useState("");
  const [takeoverTarget, setTakeoverTarget] = useState<ObservedExtension | null>(null);
  const [takeoverPreview, setTakeoverPreview] = useState<TakeoverPreview | null>(null);
  const [portableFormOpen, setPortableFormOpen] = useState(false);
  const [portablePath, setPortablePath] = useState("");
  const [exportTarget, setExportTarget] = useState<ExtensionListItem | null>(null);
  const [exportPath, setExportPath] = useState("");

  const items = workspace?.items ?? [];
  const projects = workspace?.projects ?? [];
  const projectNames = useMemo(
    () => new Map(projects.map((project) => [project.id, project.displayName])),
    [projects],
  );
  const selected = items.find((item) => item.id === selectedId) ?? null;

  useEffect(() => {
    setSelectedId(null);
    setInstallTargets([]);
    setUpdateReport(null);
    setEditView(null);
    setSkillSelection(new Set());
    setBatchReports(null);
    setBatchChecked(new Set());
  }, [kindTab]);

  const runDiscover = async () => {
    const found = await ext.discover();
    if (found) {
      setObservations(found.observations);
      setDiscoveryDiagnostics(found.diagnostics);
    }
  };

  const visible = items.filter(
    (item) =>
      item.kind === kindTab &&
      (clientFilter === "all" || matchesClient(item, clientFilter)) &&
      (search.trim() === "" ||
        searchNeedle(item).toLowerCase().includes(search.trim().toLowerCase())),
  );

  const requestInstall = async () => {
    if (!selected || installTargets.length === 0) return;
    const targets = installTargets
      .map((value) => parseTargetValue(value))
      .filter((target) => target !== null);
    if (targets.length === 0) return;
    const view = await ext.preparePlan({
      operations: [
        {
          operation: "install",
          definitionId: selected.id,
          targets,
        },
      ],
    });
    if (view) setPlanView(view);
  };

  const prepareBindingChange = async (
    binding: BindingStatus,
    enable: boolean,
    sharedSettings?: boolean,
  ) => {
    const view = await ext.preparePlan({
      operations: [
        {
          operation: enable ? "enable" : "disable",
          bindingId: binding.id,
          ...(sharedSettings === undefined ? {} : { sharedSettings }),
        },
      ],
    });
    if (view) setPlanView(view);
  };

  const changeBinding = async (binding: BindingStatus, enable: boolean) => {
    const definition = items.find((item) => item.id === binding.resourceId);
    if (
      !enable &&
      definition?.kind === "skill" &&
      binding.target.client === "claude" &&
      binding.target.scope === "projectShared"
    ) {
      setSkillDisableBinding(binding);
      setSkillDisableSharedSettings(null);
      return;
    }
    await prepareBindingChange(binding, enable);
  };

  const confirmSkillDisableScope = async () => {
    if (!skillDisableBinding || skillDisableSharedSettings === null) return;
    const binding = skillDisableBinding;
    const view = await ext.preparePlan({
      operations: [
        {
          operation: "disable",
          bindingId: binding.id,
          sharedSettings: skillDisableSharedSettings,
        },
      ],
    });
    if (view) {
      setSkillDisableBinding(null);
      setSkillDisableSharedSettings(null);
      setPlanView(view);
    }
  };

  const removeBinding = async (binding: BindingStatus) => {
    const view = await ext.preparePlan({
      operations: [{ operation: "remove", bindingId: binding.id }],
    });
    if (view) setPlanView(view);
  };

  const confirmPlan = async () => {
    if (!planView) return;
    const outcome = await ext.applyPlan(planView.planId);
    if (outcome !== null) setPlanView(null);
  };

  const requestRestore = async (operationId: string) => {
    const view = await ext.prepareRestore(operationId);
    if (view) setPlanView(view);
  };

  const runCheckUpdates = async () => {
    if (!selected || selected.kind !== "skill") return;
    const reports = await ext.checkUpdates([selected.id]);
    const report = reports?.[0];
    if (!report) return;
    if (report.error !== null) {
      toast({ kind: "error", title: "检查更新失败", description: report.error });
      return;
    }
    setUpdateReport(report);
  };

  const toggleSkillSelection = (definitionId: string, checked: boolean) => {
    setSkillSelection((previous) => {
      const next = new Set(previous);
      if (checked) next.add(definitionId);
      else next.delete(definitionId);
      return next;
    });
  };

  const runBatchCheck = async () => {
    const ids = items
      .filter((item) => item.kind === "skill" && skillSelection.has(item.id))
      .map((item) => item.id);
    if (ids.length === 0) return;
    const reports = await ext.checkUpdates(ids);
    if (!reports) return;
    setBatchReports(reports);
    setBatchChecked(
      new Set(
        reports
          .filter((report) => report.error === null && report.newDigest !== null)
          .map((report) => report.definitionId),
      ),
    );
  };

  const previewBatchUpdate = async () => {
    if (!batchReports) return;
    const checked = batchReports.filter(
      (report) => batchChecked.has(report.definitionId) && report.newDigest !== null,
    );
    if (checked.length === 0) return;
    const advanced = await ext.advanceSkillUpdates(
      checked.flatMap((report) =>
        report.newDigest !== null
          ? [{ definitionId: report.definitionId, newDigest: report.newDigest }]
          : [],
      ),
    );
    if (!advanced) return;
    if (advanced.failed.length > 0) {
      const nameOf = (id: string) => items.find((item) => item.id === id)?.name ?? id;
      toast({
        kind: "warning",
        title: "部分 Skill 内容版本未能入库",
        description: advanced.failed
          .map((failure) => `${nameOf(failure.definitionId)}：${failure.message}`)
          .join("；"),
      });
    }
    if (advanced.advanced.length === 0) return;
    const view = await ext.preparePlan({
      operations: advanced.advanced.map((definitionId) => ({
        operation: "update" as const,
        definitionId,
      })),
    });
    if (view) setPlanView(view);
  };

  const applySkillUpdate = async () => {
    if (!selected || !updateReport?.newDigest) return;
    const updated = await ext.applySkillUpdate(selected.id, updateReport.newDigest);
    if (updated) {
      setUpdateReport(null);
      if (updated.plan) setPlanView(updated.plan);
    }
  };

  const deployCurrentSkill = async () => {
    if (!selected) return;
    const view = await ext.preparePlan({
      operations: [{ operation: "update", definitionId: selected.id }],
    });
    if (view) setPlanView(view);
  };

  const startEditMcp = async () => {
    if (!selected || selected.kind !== "mcp") return;
    const view = await ext.loadMcpEdit(selected.id);
    if (view) setEditView(view);
  };

  const saveMcpEdit = async (edit: McpEditRequest): Promise<boolean> => {
    if (!editView) return false;
    const updated = await ext.applyMcpEdit(editView.id, edit);
    if (updated && updated.plan) setPlanView(updated.plan);
    return updated !== null;
  };

  const confirmRemove = async () => {
    if (!removeTarget) return;
    const id = removeTarget.id;
    const done = await ext.removeDefinition(id);
    if (done) {
      if (selectedId === id) setSelectedId(null);
      setRemoveTarget(null);
    }
  };

  const saveMcpDraft = async (draft: ExtensionDraft): Promise<boolean> => {
    const definition = await ext.saveDefinition(draft);
    if (definition) {
      setAddMode(null);
      setKindTab("mcp");
      setSelectedId(definition.id);
    }
    return definition !== null;
  };

  const createSkill = async () => {
    const definition = await ext.createSkill({
      name: newSkillName.trim(),
      description: newSkillDescription.trim(),
    });
    if (definition) {
      setAddMode(null);
      setNewSkillName("");
      setNewSkillDescription("");
      setKindTab("skill");
      setSelectedId(definition.id);
      setSkillWorkbenchId(definition.id);
    }
  };

  const saveSkillFiles = async (
    definitionId: string,
    update: Parameters<typeof ext.saveSkillFiles>[1],
  ) => {
    const saved = await ext.saveSkillFiles(definitionId, update);
    if (saved && saved.plan) setPlanView(saved.plan);
    return saved;
  };

  const forkSkill = async (definitionId: string) => {
    const definition = await ext.forkSkill(definitionId);
    if (definition) {
      setSelectedId(definition.id);
      setSkillWorkbenchId(definition.id);
    }
  };

  const restoreSkillVersion = async (definitionId: string, digest: string) => {
    const restored = await ext.restoreSkillVersion(definitionId, digest);
    if (restored && restored.plan) setPlanView(restored.plan);
  };

  const saveSkillDependencies = async (
    definitionId: string,
    update: Parameters<typeof ext.saveSkillDependencies>[1],
  ) => {
    await ext.saveSkillDependencies(definitionId, update);
  };

  const importSkillCandidate = async (
    digest: string,
    name: string,
    hostScoped: AppKind | null,
  ) => {
    const definition = await ext.importCandidate(digest, name, hostScoped);
    if (definition) {
      setAddMode(null);
      setKindTab("skill");
      setSelectedId(definition.id);
    }
    return definition;
  };

  const importObservedSkill = async (observed: ObservedExtension) => {
    if (!observed.contentDigest) {
      toast({ kind: "warning", title: "该条目没有可导入的内容摘要" });
      return;
    }
    const definition = await ext.importObservedSkill(observed.observationId);
    if (definition) {
      setAddMode(null);
      setKindTab("skill");
      setSelectedId(definition.id);
    }
  };

  const importObservedMcp = async (observed: ObservedExtension) => {
    const definition = await ext.importObservedMcp(observed.observationId);
    if (definition) {
      setAddMode(null);
      setKindTab("mcp");
      setSelectedId(definition.id);
    }
  };

  const requestTakeover = async (observed: ObservedExtension) => {
    const preview = await ext.previewTakeover(observed.observationId);
    if (preview) {
      setTakeoverTarget(observed);
      setTakeoverPreview(preview);
    }
  };

  const confirmTakeover = async () => {
    if (!takeoverTarget || !takeoverPreview) return;
    const kind = takeoverPreview.kind;
    const definition = await ext.takeoverObserved(takeoverTarget.observationId);
    if (definition) {
      setTakeoverTarget(null);
      setTakeoverPreview(null);
      setAddMode(null);
      setKindTab(kind === "mcp" ? "mcp" : "skill");
      setSelectedId(definition.id);
    }
  };

  const submitPortableImport = async () => {
    if (!portablePath.trim()) return;
    const report = await ext.importPortable(portablePath.trim());
    if (report) {
      setPortablePath("");
      setPortableFormOpen(false);
      setSelectedId(report.definition.id);
    }
  };

  const submitPortableExport = async () => {
    if (!exportTarget || !exportPath.trim()) return;
    const exported = await ext.exportPortable(exportTarget.id, exportPath.trim());
    if (exported) {
      setExportTarget(null);
      setExportPath("");
    }
  };

  const submitProject = async () => {
    if (!projectRoot.trim()) return;
    const project = await ext.addProject(projectRoot.trim());
    if (project) {
      setProjectRoot("");
      setProjectFormOpen(false);
    }
  };

  const columns: Array<TableColumn<ExtensionListItem>> = [
    ...(kindTab === "skill"
      ? ([
          {
            key: "select",
            header: "选择",
            render: (item) => (
              // The row click opens the detail; picking the checkbox must
              // stay an independent gesture.
              <span onClick={(event) => event.stopPropagation()}>
                <Checkbox
                  checked={skillSelection.has(item.id)}
                  label=""
                  ariaLabel={`选择 ${item.name}`}
                  disabled={busy}
                  onChange={(checked) => toggleSkillSelection(item.id, checked)}
                />
              </span>
            ),
          },
        ] as Array<TableColumn<ExtensionListItem>>)
      : []),
    { key: "name", header: "名称", render: (item) => item.name },
    {
      key: "origin",
      header: "来源 / 类型",
      render: (item) =>
        item.kind === "skill"
          ? (item.source ? "已记录来源" : "本机内容")
          : (item.transport === "claudeSse" ? "SSE（仅 Claude）" : item.transport === "claudeWs" ? "WebSocket（仅 Claude）" : item.transport === "http" ? "HTTP" : "stdio"),
    },
    { key: "codex", header: "Codex", render: (item) => clientSummary(item, "codex") },
    { key: "claude", header: "Claude", render: (item) => clientSummary(item, "claude") },
    {
      key: "actions",
      header: "操作",
      render: (item) => (
        <Button
          variant="secondary"
          disabled={busy}
          aria-label={`管理 ${item.name}`}
          onClick={() => setSelectedId(item.id)}
        >
          管理
        </Button>
      ),
    },
  ];

  return (
    <section className="asb-panel asb-ext" aria-label="扩展">
      <div className="asb-panel-heading">
        <h2 className="asb-panel-title">扩展</h2>
        <div className="asb-tabs" role="tablist" aria-label="扩展类型">
          <button
            id="ext-skill-tab"
            type="button"
            role="tab"
            aria-selected={kindTab === "skill"}
            aria-controls="ext-workspace-panel"
            className={`asb-tab${kindTab === "skill" ? " is-on" : ""}`}
            onClick={() => setKindTab("skill")}
          >
            Skills
          </button>
          <button
            id="ext-mcp-tab"
            type="button"
            role="tab"
            aria-selected={kindTab === "mcp"}
            aria-controls="ext-workspace-panel"
            className={`asb-tab${kindTab === "mcp" ? " is-on" : ""}`}
            onClick={() => setKindTab("mcp")}
          >
            MCP
          </button>
        </div>
      </div>
      <div
        id="ext-workspace-panel"
        role="tabpanel"
        aria-labelledby={kindTab === "skill" ? "ext-skill-tab" : "ext-mcp-tab"}
      >
        {(workspace?.recoveryRequired.length ?? 0) > 0 && (
          <div className="asb-banner asb-banner-error" role="alert" aria-label="扩展恢复告警">
            <span>
              存在未能自动恢复的扩展操作，写入已暂停：{workspace?.recoveryRequired.join("；")}
            </span>
            <Button variant="secondary" disabled={busy} onClick={() => void ext.recoverTransactions()}>
              尝试恢复
            </Button>
          </div>
        )}
        <div className="asb-ext-toolbar">
          <div className="asb-ext-filter">
            <Select
              value={clientFilter}
              options={CLIENT_FILTER_OPTIONS}
              onChange={(value) => setClientFilter(value as ClientFilter)}
              ariaLabel="客户端过滤"
              disabled={busy}
            />
          </div>
          <div className="asb-ext-search">
            <Input
              type="search"
              placeholder="搜索名称或描述"
              aria-label="搜索扩展"
              value={search}
              disabled={busy}
              onChange={(event) => setSearch(event.target.value)}
            />
          </div>
          <div className="asb-panel-actions">
            <Button variant="secondary" disabled={busy} onClick={() => setProjectFormOpen((open) => !open)}>
              注册项目目录
            </Button>
            <Button
              variant="secondary"
              disabled={busy}
              onClick={() => setPortableFormOpen((open) => !open)}
            >
              导入便携包
            </Button>
            <Button
              variant="secondary"
              disabled={busy}
              onClick={() => {
                setSelectedId(null);
                setAddMode(kindTab === "skill" ? "skillSource" : "newMcp");
              }}
            >
              {kindTab === "skill" ? "添加 Skill 来源" : "新建 MCP"}
            </Button>
          </div>
        </div>
        {projectFormOpen && (
          <ProjectRegisterForm
            busy={busy}
            projectRoot={projectRoot}
            setProjectRoot={setProjectRoot}
            submitProject={submitProject}
          />
        )}
        {portableFormOpen && (
          <PortableImportForm
            busy={busy}
            portablePath={portablePath}
            setPortablePath={setPortablePath}
            submitPortableImport={submitPortableImport}
          />
        )}
        {kindTab === "skill" && skillSelection.size > 0 && (
          <div className="asb-ext-batchbar" role="toolbar" aria-label="批量操作">
            <span className="asb-scope-note">已选择 {skillSelection.size} 个 Skill</span>
            <Button variant="secondary" disabled={busy} onClick={() => void runBatchCheck()}>
              检查更新
            </Button>
            <Button
              variant="secondary"
              disabled={busy}
              onClick={() => setSkillSelection(new Set())}
            >
              清除选择
            </Button>
          </div>
        )}
        {batchReports !== null && (
          <SkillBatchUpdatePanel
            reports={batchReports}
            checked={batchChecked}
            names={new Map(items.map((item) => [item.id, item.name]))}
            busy={busy}
            onCheckedChange={(definitionId, checked) => {
              setBatchChecked((previous) => {
                const next = new Set(previous);
                if (checked) next.add(definitionId);
                else next.delete(definitionId);
                return next;
              });
            }}
            onPreview={() => void previewBatchUpdate()}
            onClose={() => {
              setBatchReports(null);
              setBatchChecked(new Set());
            }}
          />
        )}
        {visible.length === 0 ? (
          <p className="asb-empty">
            {items.length === 0
              ? "扩展库为空；从下方入口新建或从本机导入扩展"
              : "没有符合过滤条件的扩展"}
          </p>
        ) : (
          <Table
            columns={columns}
            rows={visible}
            rowKey={(item) => item.id}
            ariaLabel="扩展列表"
            onRowClick={(item) => setSelectedId(item.id)}
          />
        )}
        {editView && selected?.id === editView.id ? (
          <McpEditForm
            envelope={editView}
            busy={busy}
            onPutSecret={ext.putSecret}
            onSave={saveMcpEdit}
            onCancel={() => setEditView(null)}
          />
        ) : selected && selected.kind === "skill" && skillWorkbenchId === selected.id ? (
          <SkillWorkbench
            item={selected}
            mcpOptions={items
              .filter((candidate) => candidate.kind === "mcp")
              .map((candidate) => ({ id: candidate.id, name: candidate.name }))}
            busy={busy}
            onLoadEditor={ext.loadSkillEditor}
            onSaveFiles={saveSkillFiles}
            onLoadVersions={ext.loadSkillVersions}
            onRestoreVersion={(definitionId, digest) => void restoreSkillVersion(definitionId, digest)}
            onSaveDependencies={saveSkillDependencies}
            onFork={(definitionId) => void forkSkill(definitionId)}
            onClose={() => setSkillWorkbenchId(null)}
          />
        ) : selected ? (
          <ExtensionDetail
            item={selected}
            projects={projects}
            capabilities={workspace?.capabilities ?? []}
            busy={busy}
            installTargets={installTargets}
            updateReport={updateReport}
            projectNames={projectNames}
            onInstallTargetsChange={setInstallTargets}
            onInstall={() => void requestInstall()}
            onChangeBinding={(binding, enable) => void changeBinding(binding, enable)}
            onRemoveBinding={(binding) => void removeBinding(binding)}
            onToggleLock={(binding, locked) => void ext.toggleBindingLock(binding.id, locked)}
            onDelete={() => setRemoveTarget(selected)}
            onEditMcp={() => void startEditMcp()}
            onEditSkillContent={
              selected.kind === "skill" ? () => setSkillWorkbenchId(selected.id) : undefined
            }
            onCheckUpdates={() => void runCheckUpdates()}
            onApplyUpdate={() => void applySkillUpdate()}
            onDeployCurrent={() => void deployCurrentSkill()}
            onExportPortable={() => {
              setExportTarget(selected);
              setExportPath("");
            }}
            onClose={() => {
              setSelectedId(null);
              setInstallTargets([]);
              setUpdateReport(null);
              setEditView(null);
              setSkillWorkbenchId(null);
            }}
          />
        ) : (
          <div className="asb-ext-add">
            <AddExtensionCards kindTab={kindTab} addMode={addMode} setAddMode={setAddMode} />
            {addMode === "newMcp" && (
              <NewMcpForm
                busy={busy}
                onPutSecret={ext.putSecret}
                onSave={saveMcpDraft}
              />
            )}
            {addMode === "skillSource" && (
              <SkillSourceForm
                busy={busy}
                onScanLocal={ext.scanLocal}
                onResolveGithub={ext.resolveSource}
                onImport={importSkillCandidate}
              />
            )}
            {addMode === "newSkill" && (
              <NewSkillForm
                busy={busy}
                newSkillName={newSkillName}
                setNewSkillName={setNewSkillName}
                newSkillDescription={newSkillDescription}
                setNewSkillDescription={setNewSkillDescription}
                createSkill={createSkill}
              />
            )}
            {addMode === "discover" && (
              <DiscoverPanel
                observations={observations}
                diagnostics={discoveryDiagnostics}
                loading={busy}
                busy={busy}
                onScan={() => void runDiscover()}
                onImportSkill={(observed) => void importObservedSkill(observed)}
                onImportMcp={(observed) => void importObservedMcp(observed)}
                onTakeover={(observed) => void requestTakeover(observed)}
              />
            )}
          </div>
        )}
        <ExtensionHistory
          records={workspace?.history ?? []}
          items={items}
          busy={busy}
          projectNames={projectNames}
          onRestore={(operationId) => void requestRestore(operationId)}
        />
      </div>
      {planView && (
        <ExtensionPlanSheet
          view={planView}
          busy={busy}
          projectNames={projectNames}
          onConfirm={() => void confirmPlan()}
          onCancel={() => setPlanView(null)}
        />
      )}
      {removeTarget && (
        <ExtensionRemoveSheet
          name={removeTarget.name}
          busy={busy}
          onConfirm={() => void confirmRemove()}
          onCancel={() => setRemoveTarget(null)}
        />
      )}
      {skillDisableBinding && (
        <SkillDisableScopeSheet
          sharedSettings={skillDisableSharedSettings}
          busy={busy}
          onSharedSettingsChange={setSkillDisableSharedSettings}
          onConfirm={() => void confirmSkillDisableScope()}
          onCancel={() => {
            setSkillDisableBinding(null);
            setSkillDisableSharedSettings(null);
          }}
        />
      )}
      {takeoverPreview && takeoverTarget && (
        <TakeoverConfirmSheet
          takeoverPreview={takeoverPreview}
          confirmTakeover={confirmTakeover}
          cancelTakeover={() => {
            setTakeoverTarget(null);
            setTakeoverPreview(null);
          }}
        />
      )}
      {exportTarget && (
        <PortableExportSheet
          busy={busy}
          exportTarget={exportTarget}
          exportPath={exportPath}
          setExportPath={setExportPath}
          submitPortableExport={submitPortableExport}
          closeExport={() => {
            setExportTarget(null);
            setExportPath("");
          }}
        />
      )}
    </section>
  );
}
