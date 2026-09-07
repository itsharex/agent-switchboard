import type {
  AppKind,
  DiagnosticSubject,
  ExtensionDiagnostic,
  ObservedExtension,
} from "../../api/client";

/** One row of the workspace filter bar, shared by the library list and the
 * discovery results: `"all"` or a concrete client. */
export type DiscoveryClientFilter = "all" | AppKind;

export interface DiscoveryViewFilter {
  /** The active workspace tab; only this resource kind is shown. */
  kind: "skill" | "mcp";
  client: DiscoveryClientFilter;
  /** Raw search text; empty means everything. */
  search: string;
}

/** Minimal facts the view needs about one library binding to decide whether
 * a managed-binding diagnostic belongs to the current view. */
export interface BindingViewInfo {
  name: string;
  kind: "skill" | "mcp";
  client: AppKind;
}

function clientMatches(client: AppKind, filter: DiscoveryViewFilter): boolean {
  return filter.client === "all" || filter.client === client;
}

function searchMatches(filter: DiscoveryViewFilter, ...texts: Array<string | null | undefined>): boolean {
  const needle = filter.search.trim().toLowerCase();
  if (needle === "") return true;
  return texts
    .filter((text): text is string => typeof text === "string")
    .some((text) => text.toLowerCase().includes(needle));
}

/** Whether one discovery row belongs to the current view. */
export function observationInView(
  observed: ObservedExtension,
  filter: DiscoveryViewFilter,
): boolean {
  return (
    observed.kind === filter.kind &&
    clientMatches(observed.client, filter) &&
    searchMatches(filter, observed.name, observed.description)
  );
}

function subjectInView(
  subject: DiagnosticSubject,
  filter: DiscoveryViewFilter,
  viewObservationIds: ReadonlySet<string>,
  bindings: ReadonlyMap<string, BindingViewInfo>,
): boolean {
  switch (subject.kind) {
    case "discoveryEntry":
      // The row is the object: it counts exactly when it is visible.
      return viewObservationIds.has(subject.observationId);
    case "managedBinding": {
      const info = bindings.get(subject.bindingId);
      if (!info) return false;
      return (
        info.kind === filter.kind &&
        clientMatches(info.client, filter) &&
        searchMatches(filter, info.name)
      );
    }
    case "scanLocation":
      // Unattributable problems stay visible under their tab and client
      // even when a search would hide every row.
      return subject.resourceKind === filter.kind;
  }
}

/** Whether one diagnostic belongs to the current view: the same tab,
 * client, and search scope the rows use — except scan-location problems,
 * which ignore the search so a filter can never hide them. */
export function diagnosticInView(
  diagnostic: ExtensionDiagnostic,
  filter: DiscoveryViewFilter,
  viewObservationIds: ReadonlySet<string>,
  bindings: ReadonlyMap<string, BindingViewInfo>,
): boolean {
  if (!clientMatches(diagnostic.client, filter)) return false;
  return subjectInView(diagnostic.subject, filter, viewObservationIds, bindings);
}
