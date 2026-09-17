import type { ProviderConnectionOptions } from "./providers";

export function usesClaudeManagedAuth(connection?: ProviderConnectionOptions | null): boolean {
  return Boolean(connection?.providerType || connection?.authBinding?.source === "managed_account");
}
