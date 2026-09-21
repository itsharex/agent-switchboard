import type { CodexProviderRecord, CodexSubagentRoute } from "../../api/client";

export function subagentModelKey(route: CodexSubagentRoute): string {
  return JSON.stringify([route.profileId, route.model]);
}

export function subagentModelOptions(records: CodexProviderRecord[], selfId?: string) {
  return records
    .filter(({ profile }) => profile.id !== selfId && !profile.connection?.authBinding)
    .flatMap(({ profile }) => profile.catalog.map(({ id }) => {
      const route = { profileId: profile.id, model: id };
      return { value: subagentModelKey(route), label: id, group: profile.name, route };
    }));
}
