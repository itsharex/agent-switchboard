<p align="center">
  <img src="src/assets/app-icon.svg" width="96" alt="Agent Switchboard logo">
</p>

<h1 align="center">Agent Switchboard</h1>

<p align="center">
  A local configuration console for <strong>Codex</strong> and <strong>Claude Code</strong>.<br>
  Manage providers, preview configuration changes, recover from switches, and track usage and probe history.
</p>

<p align="center"><sup>English · <a href="README.zh-CN.md">简体中文</a></sup></p>

Save models, connection details, and runtime parameters as provider profiles, then inspect the files and changes before switching. Client settings, Skills, MCP, session records, and usage monitoring share one desktop app.

## Download and install

Download an installer for your system and processor from [GitHub Releases](https://github.com/y4Nkk/agent-switchboard/releases). The table lists the project's packaging platforms; check the selected release's Assets for available files. This README describes the current source; consult the corresponding release notes for features in a published version.

| Platform | File to choose | Installation |
| --- | --- | --- |
| Windows x64 | `*windows-x86_64-nsis.exe` | Run the wizard; the default NSIS version installs for the current user with a selectable directory |
| macOS Apple Silicon | `*aarch64*.dmg` | Open the disk image and drag the app into Applications |
| macOS Intel | `*x86_64*.dmg` | Open the disk image and drag the app into Applications |
| Linux x64 | `*x86_64*.deb` or `*x86_64*.AppImage` | Use a package installer on Debian/Ubuntu; grant an AppImage execute permission before running it |

The custom Windows installer requires .NET Framework 4.8.1. The app requires Microsoft Edge WebView2 Runtime; the installer attempts to download and install it when missing. The Linux `.deb` depends on WebKitGTK 4.1. `Source code` archives are for development; `.sig`, `latest.json`, and updater archives belong to the update process.

## First use

Install and configure the Codex or Claude Code client you want to use, and have your provider URL, authentication details, and model names ready. Degradation radar also requires an executable local Codex CLI.

1. Create a profile in **Providers**, or import existing local configuration.
2. Set its protocol, authentication, and model. Use **Client configuration** to adjust shared settings, sub-agent runtime settings, and global instructions as needed.
3. Preview the switch, review the redacted diff, target files, and backup location, then confirm. Switching preserves configuration keys outside the profile's ownership.
4. After using the client, check consumption and quotas in **Usage**, and local conversations in **Sessions**.
5. To undo a configuration change, open **Settings → Backups → Local backups** and restore the corresponding record.

Missing configuration, syntax errors, and external edits surface specific explanations. Errors whose meaning cannot be established are never silently overwritten.

## Interface preview

These screenshots show the **actual 0.2.8 frontend**, rendered with mock data in an isolated headless browser. Providers, models, balances, usage, and probe records are fictional demonstrations, not measurements or claims about real services. Capture does not contact model services or read personal configuration or credentials. The screenshots show the Chinese interface.

**Provider overview** — See the active Codex and Claude Code connections, manage profiles, and check balances.

![Provider overview with active connections, models, profiles, and simulated balances](docs/screenshots/providers.png)

<details>
<summary>Switch preview: review changes before applying</summary>

Inspect changes to models, endpoints, and runtime parameters, together with the target configuration file and backup location.

![Switch preview showing changed keys, configuration content, and backup location](docs/screenshots/switch-preview.png)

</details>

<details>
<summary>Client configuration: sub-agents, sandbox, and approvals</summary>

Set sub-agent availability and concurrency, and inspect the current sandbox and approval settings.

![Client configuration with sub-agent runtime and sandbox approval settings](docs/screenshots/client-configuration.png)

</details>

<details>
<summary>Usage: model shares and daily trends</summary>

Review input, cached, and output tokens by time range, with a model breakdown, daily trends, and session counts.

![Simulated token usage with model shares, seven-day trends, and detailed counts](docs/screenshots/usage.png)

</details>

<details>
<summary>Degradation radar: batch results and individual runs</summary>

Review pass counts, token consumption, reasoning tokens, final answers, and duration for each run. The pictured results are simulated.

![Degradation radar with five simulated probe results and token counts](docs/screenshots/radar.png)

</details>

<details>
<summary>Probe history: filter by time, profile, and status</summary>

Browse the configuration, question, pass count, and consumption recorded for each batch, then open its details.

![Simulated probe history with time, profile, and status filters](docs/screenshots/radar-history.png)

</details>

<details>
<summary>Local gateway: protocol topology and request status</summary>

Inspect the loopback address, protocol translation paths, request counts, failures, and latency distribution.

![Local gateway with simulated protocol translation topology and request metrics](docs/screenshots/gateway.png)

</details>

## Core features

| Location | What you can do |
| --- | --- |
| Providers | Create, edit, reorder, import, and delete profiles; manage models, authentication, protocols, model mappings, and runtime parameters; diagnose configuration and connections, preview repairs and undo them; export provider SQL files |
| Client configuration | Manage shared settings, Codex sub-agent runtime settings, and global instructions; preview and apply drafts, including controlled edits to fields outside the regular form |
| Extensions | Manage Skills and MCP with per-client enable/disable, search, updates, import/export, local discovery, and advanced management |
| Sessions | Full-text search and message navigation, project groups, pins, tags, local aliases, time filters, and bulk organization; resume or delete sessions, export Markdown, and save prompts or answers as snippets |
| Usage | Inspect consumption, quotas, and reset times; run degradation radar and browse probe history |
| Settings → Client tools | Save and restore Codex work scenarios; manage Claude client integration |
| Settings → Preferences | Follow the system, Simplified Chinese, or English for the interface; select 90%, 100%, 110%, or 125% interface scale, record a global shortcut, and choose the startup page |
| Settings → Local gateway / Diagnostics | Inspect gateway status, configuration and environment issues, and runtime logs |

**Interface language.** The whole desktop app — main window, tray panel, notifications, dialogs, and app-owned error explanations — ships in Simplified Chinese and English. The preference offers 跟随系统 / 简体中文 / English and is saved like every other application preference: switching applies to the main window and tray immediately, survives a restart, and never touches user content (provider names, session text, configuration originals, and third-party diagnostics stay verbatim). A fresh install follows the system locale — a Chinese desktop gets Simplified Chinese, everything else gets English — while an existing installation upgraded from an earlier release keeps Chinese. Dates, numbers, and quota window names follow the chosen language with identical precision and business meaning.

Codex work scenarios save combinations of existing providers, extensions, and instructions, with a change preview before restoration. The global shortcut shows and focuses the main window, or hides it to the tray when already focused. Startup can open the provider list or the last visited top-level page, without restoring editing drafts. Unreadable preferences require explicit repair, which resets only application preferences.

Provider switches and configuration draft writes require preview and confirmation, with backup and recovery. Extension operations apply immediately by default; sensitive connection data, deleting definitions with existing installs, and disabling a Claude project-shared Skill require extra confirmation.

### Session search and saved snippets

Session search reads current local transcripts, including message bodies, titles, summaries, project paths, and session IDs. Submit a phrase, error, or code fragment; matching ignores case and collapses whitespace, including line breaks. Results are paginated with message excerpts. Opening a match reloads the source and expands and focuses the matching message; changed or removed messages are reported instead of jumping to a different message. Unreadable sources are reported alongside available results.

Export Markdown uses the system save dialog and includes the complete readable transcript and source metadata. Save a user or assistant message to keep an independent local snapshot in **Saved snippets**, where it can be searched, read, copied, removed, or opened in its source conversation. Deleting a session does not delete its saved snippets. These snapshots live in `state/sessions/collections.sqlite3` inside the app data directory and are not included in provider exports or cloud backups. A damaged or unexpected collection database is reported and preserved, never silently reset.

Group sessions by client and project, filter by last activity, tags, or pinned status, and open a search result's project directly. Pins, tags, and local aliases are stored in `state/sessions/organization.sqlite3` inside the app data directory; client transcripts stay unchanged. Bulk actions pin, unpin, add tags, or remove tags. Filtering and sorting happen before pagination, and page selection deduplicates sessions.

### Provider diagnostics and repair

Open a saved provider's details and select **Diagnose**. Local checks cover the endpoint, protocol, authentication configuration, model, environment variables, client configuration, and gateway routing. Environment checks report variable names and their inspection scope without exposing secret values. Manually run connectivity checks or real model requests to verify the upstream service; model requests may consume provider quota. Direct upstream tests do not verify the client's complete path through the gateway.

Configuration repair applies only to the currently active saved profile. Review the changes before confirming; profile-owned fields are regenerated through the existing switch executor with backup and recovery. Undo previews are tied to that repair and reject later configuration changes. Persistent backups remain available in **Settings → Backup recovery** after closing the diagnostic window. Inactive profiles, environment overrides, and upstream failures receive guidance without silently switching providers or guessing credentials.

Generated immutable model catalogs are retained; undo restores configuration and authentication. A repair that only recreates a missing catalog explains this outcome and does not offer an empty undo action.

When provider storage fails validation, **Repair provider data** fills only missing automatic setting intents or removes a leading JSON BOM. It validates a complete staged copy before activation and retains the original configuration directory. Ambiguous files, retired layouts, and pending transactions are reported with their paths and left unchanged. This action does not clear profiles or edit live Codex or Claude Code configuration.

### Quota cache and degradation radar

Provider rows keep the model on a fixed primary line and show every returned usage window below it. Balances, reset countdowns, validity, last-update time, and refresh are visible without opening the detailed table. The tray reads the same cache, shows up to two readings that fit, and marks additional readings with `+N`. Failed refreshes retain the last reading with an explicit stale label; opening either view does not trigger a network query.

Usage scripts return one reading or an array. Numeric fields are `remaining`, `used`, `total`, with `unit` (`%` for percentage readings); optional metadata is `planName`, `resetsAt` (RFC 3339 with timezone), `isValid`, `invalidMessage` (only when invalid), and `extra` (plain text). Missing numbers remain unknown. Native CC Switch import templates preserve declared reset times and percentage units; stored custom scripts are never silently rewritten.

Usage details default to collapsed. The current strict preferences contract stores explicit expansions in `expandedUsageIds`; the former collapsed-id field is rejected, without compatibility reads or automatic preference rewriting. An existing preferences file in the old shape requires the application's explicit preference repair action. Disposable usage caches in an invalid shape are rebuilt by the next configured or manual query.

Provider balances and Codex official quotas refresh in the background at the interval set for each profile, including while on another page or hidden in the tray. Lists and the tray share the cache; opening a page does not trigger another query. Set the interval to `0` for manual queries only.

In **Usage → Degradation radar**, batch-test the active Codex configuration with built-in or custom questions:

- **Results:** Only successfully completed answers are graded against the final nonnegative integer answer. Failed, cancelled, and interrupted runs remain unjudged. Changes to configuration or the effective route during a probe terminate the current call.
- **Consumption:** Probes make real provider calls and consume quota; their usage is included in statistics. Reasoning tokens and consumption come from that CLI session. Missing data stays unknown, and partial totals disclose their coverage. Results are reference signals and cannot establish the actual model's identity.
- **History:** Questions, expected answers, results, CLI versions, and configuration snapshots are stored locally without credentials. They remain available after refresh or restart. Profile renames and deletions do not rewrite history. An abrupt exit retains completed results and marks the batch interrupted; restarting does not resume it automatically.
- **History actions:** Filter by time, profile, or status, browse pages and details, and rerun a stored question with its answer and repetition count. Reruns make new calls; deletion affects radar records only. Failed result saves show the cause and block new probes and normal exit until saving succeeds.

### Sub-agent models and protocol gateway

A Codex default sub-agent model can target a model in another saved profile that is not account-bound. Requests use that target's authentication, protocol, model mapping, and runtime parameters, and usage belongs to the target. Invalid references fail explicitly, with no fallback to the main model. Referenced profiles and models cannot be deleted. Editing a target also previews and updates any affected active model catalog.

When protocol translation or sub-agent routing is needed, the app uses a gateway listening only on `127.0.0.1`. Keep the app running while using those routes.

| Client | Native upstream | Translated upstreams |
| --- | --- | --- |
| Codex (Responses) | Responses | Chat Completions, Anthropic Messages |
| Claude Code (Anthropic Messages) | Anthropic Messages | Chat Completions, Responses, Gemini |

Translation has explicit limits: fields and tools that cannot be represented losslessly fail, while some metadata affecting only metering or caching is dropped after validation. Cross-protocol Codex requests require `store=false`, with continuation context filled from local history; switching profiles or keys invalidates old encrypted continuations. Codex WebSocket connections terminate at the gateway and use HTTP/SSE upstream. Sub-agent routes support `/responses/compact` and V2 Responses compaction according to the target's capabilities; other auxiliary operations reject cross-provider model references. Claude failover follows only the policy explicitly configured in the local `claude-failover.json`.

## Data storage and scope

Configuration, caches, sessions, probe history, and diagnostics are stored locally by default. The app manages Codex and Claude Code only, with no telemetry, application account system, automatic cloud sync, public proxy, or implicit provider switching. Network features such as model requests, quota queries, and extension downloads contact their respective services. Optional cloud backups require user setup and upload confirmation.

| Method | What it saves and restores |
| --- | --- |
| Local file backups | File backups created by configuration writes, for restoring the corresponding changes from backup records |
| Provider SQL export | Complete configuration of all provider profiles, for import within the app on another device; files may contain authentication details and should be handled as credentials |
| Encrypted cloud backup | Provider profiles, client settings, and switch records from the application's configuration store; excludes local sessions, radar history, and local file backups |

Open **Settings → Backups → Encrypted cloud backup** and follow the in-app guide to configure your own Supabase project, Publishable key, and project Auth user. A separate backup password encrypts the backup on this device before upload. Each upload replaces that account's existing cloud backup. Login and backup passwords are not persisted; restoration requires the original backup password.

Cloud restoration replaces the application's provider profiles, client settings, and switch records. It does not directly change the active Codex / Claude Code configuration or delete local file backups. Activating a restored profile still requires a preview and confirmed switch.

## Development and builds

### Prerequisites

Use Node.js **22.12+ (22.x)**, npm, and Rust. See the [Tauri platform prerequisites](https://v2.tauri.app/start/prerequisites/) for system dependencies, with the following project-specific toolchain and installer requirements.

| Platform | Project requirements |
| --- | --- |
| Windows x64 | `stable-x86_64-pc-windows-gnu`; MinGW-w64 GCC and binutils, with `gcc` and `windres` on `PATH`; WebView2 Runtime |
| macOS | Rust `stable`, Xcode Command Line Tools; 7-Zip for the packaging environment (CI uses `p7zip`) |
| Linux | Rust `stable`; WebKitGTK 4.1, build tools, and development libraries; see the Ubuntu command below |

On Windows, install `mingw-w64-x86_64-gcc` and `mingw-w64-x86_64-binutils` through MSYS2 and add its `mingw64/bin` to `PATH`. Building the custom installer also requires PowerShell 7 (`pwsh`), MSBuild, and the .NET Framework 4.8.1 targeting pack with WPF reference assemblies.

The repository's `rust-toolchain.toml` pins Windows GNU. **On macOS/Linux, override it with the host toolchain in each development or packaging shell**, as CI does:

```bash
rustup toolchain install stable
export RUSTUP_TOOLCHAIN=stable
```

Ubuntu system dependencies used by CI:

```bash
sudo apt-get update
sudo apt-get install -y libwebkit2gtk-4.1-dev build-essential curl wget file \
  libxdo-dev libssl-dev librsvg2-dev
```

### Run locally

Install dependencies and start desktop development from the repository root:

```bash
npm ci
npm run dev:desktop
```

| Command | Purpose |
| --- | --- |
| `npm run dev:desktop` | Desktop app and local backend; Rust development output goes to `target-dev/` |
| `npm run dev` | Browser development with the same local backend; frontend at `http://127.0.0.1:1420` |
| `npm run dev:frontend` | Vite only; running it alone does not provide the full local business functionality |
| `npm run build` | TypeScript checking and a production frontend build in `dist/` |
| `npm run typecheck` | TypeScript checking only |

Development uses the real local backend. Use isolated directories when verifying configuration writes, and follow [AGENTS.md](AGENTS.md) before running checks. There is currently no `test` script in `package.json`.

### Build installers

Run on the corresponding target operating system:

| Platform / installation scope | Command |
| --- | --- |
| Windows, current user, selectable directory | `npm run tauri:build:windows` |
| Windows, all users, Program Files | `npm run tauri:build:windows:msi` |
| macOS | `npm run tauri:build:macos` |
| Linux | `npm run tauri:build:linux` |

The two Windows commands wrap NSIS or MSI engines in a custom `.exe` installer under `target/release/bundle/installer/`. macOS and Linux packages are in format-specific directories under `target/release/bundle/`. Setting `CARGO_TARGET_DIR` changes the output root accordingly.

The workspace version in [Cargo.toml](Cargo.toml) owns the application version. See [package.json](package.json) for all scripts and the [packaging workflow](.github/workflows/package.yml) for the platform matrix and release steps. Local builds do not automatically publish a release.

### Local build caches and verification data

Development and test builds use smaller debug information and disable incremental compilation to slow growth in `target/` and `target-dev/`; the release profile is unchanged. Set `CARGO_PROFILE_DEV_DEBUG=full` for a single build when variable-level debugging is needed. Small Rust edits may take longer to rebuild without incremental compilation.

On Windows, `npm run prune:local` previews cache sizes and eligible stale directories. Close related verification windows and build processes, review the list, then run `npm run prune:local -- -Apply` to remove them. By default, known verification browser profiles have a three-day protection window and a roughly 0.5 GiB budget; Rust development directories and release intermediates each have an 8 GiB cleanup threshold and a one-day protection window. Cleanup runs only when explicitly invoked, so build-time peak usage has no hard cap. Screenshots, verification scripts, release executables, and installers are excluded.

## Project documentation

| Document | Contents |
| --- | --- |
| [README.md](README.md) / [简体中文](README.zh-CN.md) | Current product capabilities, usage, and development entry points |
| [DESIGN.md](DESIGN.md) | Visual, interaction, layout, and accessibility contracts |
| [CHANGELOG.md](CHANGELOG.md) | Version change history |
| [AGENTS.md](AGENTS.md) | Contribution rules, change scope, and verification constraints |

## License

Source code and documentation written for Agent Switchboard in this repository are released under the [MIT License](LICENSE). Third-party dependencies and data remain under their respective licenses; this license grants no rights to third-party trademarks.

`Codex`, `Claude Code`, and related trademarks belong to their respective owners. Agent Switchboard is not affiliated with, endorsed by, or connected to OpenAI or Anthropic.
