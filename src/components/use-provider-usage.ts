import { useCallback } from "react";
import { queryProfileUsage, readProfileUsage, type AppKind, type UsageQuery } from "../api/client";
import { useCachedQuery } from "./use-cached-query";

export function useProviderUsage(profile: { app: AppKind; id: string; usageQuery?: UsageQuery | null }) {
  const read = useCallback((id: string) => readProfileUsage(profile.app, id), [profile.app]);
  const query = useCallback((id: string) => queryProfileUsage(profile.app, id), [profile.app]);
  const cached = useCachedQuery(profile.id, JSON.stringify(profile.usageQuery) ?? "null", read, query);
  return {
    ...cached,
    data: cached.data?.summary ?? null,
    error: cached.error ?? cached.data?.error ?? null,
  };
}

export type ProviderUsage = ReturnType<typeof useProviderUsage>;
