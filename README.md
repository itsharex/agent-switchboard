<p align="center">
  <img src="src/assets/app-icon.svg" width="96" alt="Agent Switchboard logo">
</p>

<h1 align="center">Agent Switchboard</h1>

<p align="center">
  A local configuration console for <strong>Codex</strong> and <strong>Claude Code</strong>.<br>
  Provider profiles, typed previews, and resumable switches — instead of hand-editing client config files.
</p>

<p align="center"><sup>English · <a href="README.zh-CN.md">简体中文</a></sup></p>

Agent Switchboard brings provider profiles, client configuration, extension management, and local status together in a single desktop app. Real configuration is written only by the one switch executor after an explicit confirmation; every resumable write is observable, backed up, validated, and recoverable.

## Current UI (isolated demo data)

<p align="center">
  <img src="docs/screenshots/providers.png" width="100%" alt="Agent Switchboard providers workspace: current Codex and Claude Code connection cards, three README sandbox provider profiles, and the apply action">
</p>

<p align="center"><sub>Providers workspace: rendered by the current source, showing the applied “README Sandbox · Primary Route” profile and two fictional profiles ready to switch to.</sub></p>

<p align="center">
  <img src="docs/screenshots/client-configuration.png" width="100%" alt="Agent Switchboard client configuration workspace: Codex shared settings and controlled configuration actions">
</p>

<p align="center"><sub>Client configuration: reads and edits controlled settings inside an isolated client root, keeping the grouping, status, and action structure of the real UI.</sub></p>

<p align="center">
  <img src="docs/screenshots/switch-preview.png" width="100%" alt="Agent Switchboard provider switch confirmation: desensitized configuration diff, candidate files, and confirm actions for the README sandbox backup profile">
</p>

<p align="center"><sub>Switch preview: a real typed preview generated for “README Sandbox · Backup”; nothing is written to the isolated sandbox until you confirm.</sub></p>

> **Screenshot data notice**: Screenshots are produced by the actual frontend launched from the current source together with the same Tauri local backend. The run redirects `APPDATA`, `LOCALAPPDATA`, `USERPROFILE`, `CODEX_HOME`, and `CLAUDE_CONFIG_DIR` to isolated temporary directories; profile names, models, key placeholders, service addresses, and configuration contents are fictional README sandbox data (only `*.sandbox.example` / `example.com` are used). No real user Codex / Claude Code configuration, credentials, accounts, sessions, service addresses, or files were read, written, or captured.

## Product scope

- Manages Codex and Claude Code only.
- All configuration, backups, history, and diagnostics stay on this machine.
- When a third-party upstream protocol does not match the client protocol, the app starts a local translation gateway on `127.0.0.1` only.
- No cloud sync, telemetry, account system, public proxy, auto-proxying, or implicit provider switching.

## Core capabilities

### Provider profiles

- Create, edit, reorder, import, and delete Codex and Claude Code provider profiles.
- Export the complete configuration of every provider into a single SQL file, then pick that file in the app on another device to import — no command line involved.
- Store models, authentication, request protocol, request mode, model mappings, and run parameters.
- Generate a typed preview before anything is written; real writes go through an executor that is observable, backed up, validated, and recoverable.
- Client configuration keys that do not belong to the current profile are preserved, so existing user settings are never overwritten.

### Client configuration

- Manage shared configuration for both clients, Codex sub-agent run settings, and each client's global instructions file.
- Controlled fields are always normalized from UI state: explicit values are written, automatic values are removed; known historical fields are cleaned up within the same resumable transaction, unknown fields are left untouched.
- Configuration drafts can read desensitized real local configuration; fields the UI does not own can be changed in the controlled manual editor, with sensitive markers preserving the original value in the backend — every write requires preview, confirmation, backup, and validation.
- When a configuration file is malformed, the app only offers provably safe repair candidates; errors whose semantics cannot be established are never silently overwritten.
- Global instructions are edited directly in the corresponding user-level file, managed alongside the same client's shared configuration.

### Client tools

- Keeps only the two client-specific features that have no other page of their own: Codex workspaces and Claude client integration.
- A Codex workspace only saves and restores combinations of existing providers, extensions, and instructions; it never creates a second configuration set or re-manages those resources.
- No connection status, cross-page shortcuts, or duplicated content from providers, shared configuration, gateway, usage, sessions, diagnostics, MCP, and Skills.

### Extensions

- Manage Skills and MCP services with per-client enable/disable, search, update, import/export, and local discovery.
- The primary click on a list row goes straight to editing; deploy, diagnostics, project install, connection checks, capability audits, and delete are concentrated in a separate advanced management panel.
- Extension writes apply immediately by default; only sensitive connection data, deleting definitions that still have installs, and deactivating a Claude project-shared skill ask for extra confirmation.

### Local protocol gateway

- Codex (Responses) converts between Chat Completions and Anthropic Messages upstreams and passes Responses upstreams through; Claude (Anthropic Messages) converts between Chat Completions, Responses, or Gemini upstreams and passes Anthropic upstreams through.
- Fields and tools that cannot be expressed losslessly fail the request before forwarding instead of being dropped silently: for example `stop_sequences` to a Responses upstream, strict tools and audio to an Anthropic upstream, and server-side tools such as `web_search` to any cross-protocol upstream. Pure metadata that only affects metering or caching (such as `cache_control`, and `metadata.user_id` on the Gemini upstream) is dropped after validation.
- Cross-protocol Codex requests must use `store=false`; `previous_response_id` and short continuations are backfilled into full context from the local tool history, while native Responses upstreams keep using the upstream's own storage.
- Reasoning traces round-trip as encrypted continuation payloads bound to their route; after switching profiles or keys, old continuations are rejected.
- Codex WebSocket transport terminates at the gateway and upstreams uniformly use HTTP/SSE; Claude failover follows only the explicit policy in the local `claude-failover.json` with circuit-breaker cooldowns — no implicit switching.

### Status, recovery, and diagnostics

- Current connections, usage, quota, sessions, backups, logs, configuration status, and gateway diagnostics.
- The usage page offers a “degradation radar”: it batch-tests the currently active configuration through the local Codex CLI with built-in or custom questions, showing per-run pass results, reasoning tokens, and actual consumption; probe sessions are honestly counted in usage statistics, and the verdict is a reference signal, not a model judgment.
- Every resumable write creates a record; results can be inspected and restored from the operation history.
- External edits, missing configuration, syntax errors, file replacements, and failed restores all surface explicit status — nothing is silently overwritten or fabricated.

## Getting started

1. Create a provider profile, or import from existing local configuration.
2. Fill in models and connection details; adjust client configuration, sub-agent settings, or global instructions as needed.
3. Open the typed preview, check the desensitized diff, candidate files, and backup location, then confirm the apply.
4. If the result is not what you expected, restore from the matching backup in the operation history.

## Running from source

With the [Tauri platform prerequisites](https://v2.tauri.app/start/prerequisites/), Node.js, and Rust ready, run from the repository root:

```bash
npm ci
npm run dev:desktop
```

The custom Windows installer is built by `installer/AgentSwitchboard.Installer.csproj`; building it with `npm run tauri:build:windows` (NSIS engine, per-user install with a selectable directory) or `npm run tauri:build:windows:msi` (MSI engine, all-users install to Program Files) additionally requires MSBuild and the .NET Framework 4.8.1 targeting pack (Visual Studio or Visual Studio Build Tools). For frontend development:

```bash
npm run dev:frontend
```

Build, type-check, and test scripts are defined in [`package.json`](package.json). Follow the scope and verification rules in [`AGENTS.md`](AGENTS.md) before running them.

## Project documentation

| Document | Contents |
| --- | --- |
| [DESIGN.md](DESIGN.md) | Current visual, interaction, layout, and accessibility contracts |
| [CHANGELOG.md](CHANGELOG.md) | Release notes |
| [AGENTS.md](AGENTS.md) | Contribution rules, product boundaries, and verification constraints |

## License

Source code and documentation written for Agent Switchboard in this repository are released under the [MIT License](LICENSE). Third-party dependencies and data remain under their respective licenses; this license grants no rights to third-party trademarks.

`Codex`, `Claude Code`, and related trademarks belong to their respective owners. Agent Switchboard is not affiliated with, endorsed by, or connected to OpenAI or Anthropic.
