import type { CommandError } from "../../api/client";
import type { ExclusiveRunner, WorkspaceRefresher } from "./extension-ops";
import { useDiscoveryImport } from "./useDiscoveryImport";
import { useDiscoverySnapshot } from "./useDiscoverySnapshot";

interface DiscoverScanDeps {
  refresh: WorkspaceRefresher;
  runExclusive: ExclusiveRunner;
  onError: (error: CommandError) => void;
}

/** A scan owns observation identity; batch import rescans after its last item. */
export function useDiscoverScan({ refresh, runExclusive, onError }: DiscoverScanDeps) {
  const state = useDiscoverySnapshot(onError);
  const importSelected = useDiscoveryImport({ refresh, scan: state.scan, runExclusive });
  return { ...state, importSelected };
}

export type DiscoverScan = ReturnType<typeof useDiscoverScan>;
