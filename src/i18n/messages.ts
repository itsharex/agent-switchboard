/**
 * The single message catalog. Every domain file contributes its `messages`
 * and each entry is a [zh-CN, en-US] pair, so both languages are defined
 * together and stay key-identical by construction. `MessageKey` is derived
 * from this object: translating an unknown key is a type error, and each
 * domain owns a reserved key prefix so names cannot collide silently.
 */
import { messages as backup } from "./messages/backup.ts";
import { messages as clientConfig } from "./messages/clientConfig.ts";
import { messages as cmdConfig } from "./messages/cmd-config.ts";
import { messages as cmdExtlib } from "./messages/cmd-extlib.ts";
import { messages as cmdExtops } from "./messages/cmd-extops.ts";
import { messages as cmdMisc } from "./messages/cmd-misc.ts";
import { messages as cmdSwitch } from "./messages/cmd-switch.ts";
import { messages as codex } from "./messages/codex.ts";
import { messages as common } from "./messages/common.ts";
import { messages as errors } from "./messages/errors.ts";
import { messages as extcap } from "./messages/extcap.ts";
import { messages as extensions } from "./messages/extensions.ts";
import { messages as format } from "./messages/format.ts";
import { messages as gateway } from "./messages/gateway.ts";
import { messages as importDiscovery } from "./messages/importDiscovery.ts";
import { messages as mcp } from "./messages/mcp.ts";
import { messages as operations } from "./messages/operations.ts";
import { messages as ownership } from "./messages/ownership.ts";
import { messages as providers } from "./messages/providers.ts";
import { messages as providerDiagnostics } from "./messages/provider-diagnostics.ts";
import { messages as providerDiagnosticsBackend } from "./messages/provider-diagnostics-backend.ts";
import { messages as sessions } from "./messages/sessions.ts";
import { messages as settings } from "./messages/settings.ts";
import { messages as tray } from "./messages/tray.ts";
import { messages as usage } from "./messages/usage.ts";

export const messages = Object.freeze({
  ...backup,
  ...clientConfig,
  ...cmdConfig,
  ...cmdExtlib,
  ...cmdExtops,
  ...cmdMisc,
  ...cmdSwitch,
  ...codex,
  ...common,
  ...errors,
  ...extcap,
  ...extensions,
  ...format,
  ...gateway,
  ...importDiscovery,
  ...mcp,
  ...operations,
  ...ownership,
  ...providers,
  ...providerDiagnostics,
  ...providerDiagnosticsBackend,
  ...sessions,
  ...settings,
  ...tray,
  ...usage,
} as const);

export type MessageKey = keyof typeof messages;
export type { MessageEntry, MessageParams, ResolvedLanguage } from "./types";
