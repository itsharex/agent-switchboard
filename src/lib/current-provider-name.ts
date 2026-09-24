import type { AppKind, ConfigFileStatus } from "../api/client";
import { tr } from "../i18n/current.ts";

/** The minimal profile shape the active-route lookup needs; satisfied by the
 * generic profile store and by Codex third-party records alike. */
export interface ActiveProfileRef {
  app: AppKind;
  id: string;
  name: string;
  websiteUrl: string | null;
}

/**
 * Provider identity comes from the live connection, not configuration drift
 * or the profile mentioned by a historical write.
 */
export function currentProviderProfile(
  status: ConfigFileStatus | undefined,
  candidates: readonly ActiveProfileRef[],
): ActiveProfileRef | null {
  if (!status?.route) return null;
  return candidates.find(
    (profile) => profile.app === status.app && profile.id === status.activeProfileId,
  ) ?? null;
}

export function currentProviderName(
  status: ConfigFileStatus | undefined,
  candidates: readonly ActiveProfileRef[],
): string {
  if (!status?.route) return tr("format.provider.notLoaded");

  const active = currentProviderProfile(status, candidates);
  if (active) return active.name;

  if (status.route.providerName) return status.route.providerName;
  if (status.route.routeMode === "official") return tr("format.provider.officialLogin");
  return tr("format.provider.unrecognized");
}
