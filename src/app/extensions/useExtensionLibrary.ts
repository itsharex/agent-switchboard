import { createElement, useCallback } from "react";
import {
  deleteExtension,
  getMcpEditView,
  recoverExtensionTransactions,
  registerProject,
  saveExtension,
  setBindingLock,
  updateMcpDefinition,
  putExtensionSecret,
  type ExtensionDraft,
  type McpEditRequest,
  type McpEditViewEnvelope,
  type ProjectRegistration,
} from "../../api/client";
import { toast, toastMessage } from "../../components/use-toast";
import { ToastMessageList } from "../../components/Toaster";
import type {
  EditPreparation,
  ExclusiveRunner,
  ExtensionReader,
  WorkspaceRefresher,
} from "./extension-ops";
import { definitionUpdatePreparation, refreshLibraryWrite } from "./extension-ops";

interface LibraryDeps {
  refresh: WorkspaceRefresher;
  runExclusive: ExclusiveRunner;
  runRead: ExtensionReader;
}

function useDefinitionMutations({ refresh, runExclusive }: LibraryDeps) {
  const saveDefinition = useCallback(
    (draft: ExtensionDraft) =>
      runExclusive(async () => {
        const definition = await saveExtension(draft);
        await refreshLibraryWrite(refresh, { kind: "info", title: toastMessage("extensions.libraryOp.saved") });
        return definition;
      }),
    [refresh, runExclusive],
  );

  const applyMcpEdit = useCallback(
    (definitionId: string, edit: McpEditRequest): Promise<EditPreparation | null> =>
      runExclusive(async () => {
        const definition = await updateMcpDefinition(definitionId, edit);
        return definitionUpdatePreparation(definition, await refresh());
      }),
    [refresh, runExclusive],
  );

  const removeDefinition = useCallback(
    (id: string) =>
      runExclusive(async () => {
        await deleteExtension(id, true);
        await refreshLibraryWrite(refresh, { title: toastMessage("extensions.libraryOp.deleted") });
        return true;
      }),
    [refresh, runExclusive],
  );
  return { saveDefinition, applyMcpEdit, removeDefinition };
}

function useExtensionMaintenance({ refresh, runExclusive }: LibraryDeps) {
  const recoverTransactions = useCallback(
    () => runExclusive(async () => {
      const results = await recoverExtensionTransactions();
      const workspace = await refresh();
      toast({
        kind: workspace !== null && workspace.recoveryRequired.length === 0 ? "success" : "warning",
        title: workspace === null
          ? toastMessage("extensions.libraryOp.recoverRefreshFailed")
          : workspace.recoveryRequired.length > 0
            ? toastMessage("extensions.libraryOp.recoverRemaining")
            : toastMessage("extensions.libraryOp.recoverDone"),
        description: results.length > 0 ? createElement(ToastMessageList, { items: results })
          : toastMessage("extensions.libraryOp.nothingToRecover"),
      });
      return results;
    }),
    [refresh, runExclusive],
  );
  const toggleBindingLock = useCallback(
    (bindingId: string, locked: boolean) =>
      runExclusive(async () => {
        await setBindingLock(bindingId, locked);
        await refreshLibraryWrite(refresh, {
          title: locked ? toastMessage("extensions.libraryOp.locked") : toastMessage("extensions.libraryOp.unlocked"),
          description: locked
            ? toastMessage("extensions.libraryOp.lockedBody")
            : toastMessage("extensions.libraryOp.unlockedBody"),
        });
        return true;
      }),
    [refresh, runExclusive],
  );

  const addProject = useCallback(
    (root: string) =>
      runExclusive(async (): Promise<ProjectRegistration> => {
        const project = await registerProject(root);
        await refreshLibraryWrite(refresh, {
          title: toastMessage("extensions.libraryOp.projectRegistered"),
          description: project.displayName,
        });
        return project;
      }),
    [refresh, runExclusive],
  );
  return { recoverTransactions, toggleBindingLock, addProject };
}

/** Definition writes remain separate from the client's prepare/apply transaction. */
export function useExtensionLibrary(deps: LibraryDeps) {
  const { runRead, runExclusive } = deps;
  const definitions = useDefinitionMutations(deps);
  const maintenance = useExtensionMaintenance(deps);
  const loadMcpEdit = useCallback(
    (definitionId: string) => runRead((): Promise<McpEditViewEnvelope> => getMcpEditView(definitionId)),
    [runRead],
  );
  const putSecret = useCallback(
    (value: string, purpose: string) =>
      runExclusive(async (): Promise<string> => {
        const { reference } = await putExtensionSecret(value, purpose);
        return reference;
      }),
    [runExclusive],
  );

  return {
    ...definitions,
    ...maintenance,
    loadMcpEdit,
    putSecret,
  };
}
