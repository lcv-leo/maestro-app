# Runtime Bootstrapper

Status: implementation contract.
Date: 2026-04-26.

Maestro must be able to prepare a Windows 11+ machine for full operation on first run.

The bootstrapper is a guided background installer/configurator. It does not open random terminal windows. It presents checks, proposed actions, progress, output, prompts, and required operator interventions inside Maestro's own UI.

## First-Run Flow

1. Detect the runtime environment.
2. Build a dependency inventory.
3. Classify each dependency as `ready`, `missing`, `outdated`, `misconfigured`, `auth_required`, or `manual_action_required`.
4. Propose the safest install/update/configuration plan.
5. Ask the operator for explicit authorization before changing the system.
6. Execute approved actions in background.
7. Stream redacted stdout/stderr and progress into Maestro UI.
8. Pause for operator input when login, browser authorization, MFA, license acceptance, or elevated OS permission is required.
9. Re-run verification after every action.
10. Persist a local JSON/NDJSON bootstrap report under ignored runtime data.

No package, CLI, credential, login, or configuration change may be installed silently without operator approval.

## Dependency Classes

Required for runtime:

- WebView2 Runtime.
- Network access.
- Local writable app folder.
- Claude CLI.
- Codex CLI.
- Gemini CLI.
- DeepSeek API credential when DeepSeek is enabled as an editorial peer.
- Cloudflare API credential validation when D1 import/export is enabled.
- Cloudflare Wrangler / Cloudflare CLI as D1 fallback tooling.

Required for development or advanced local builds:

- Git.
- Node.js LTS/current baseline required by the app.
- npm.
- Rustup and stable MSVC Rust toolchain.
- Visual Studio Build Tools with C++ workload when native builds are needed.

Optional capability dependencies:

- PDF extraction/rendering helpers when native libraries are selected.
- Browser drivers or WebView helpers for rendered evidence collection.
- Package managers: `winget`, `scoop`, `choco`, npm global installs, rustup, vendor installers.
- Windows PATH configuration helpers.
- PowerShell execution policy diagnostics and scoped adjustments when required.

## Package Manager Preference

On Windows 11+, Maestro should prefer:

1. Official vendor installers or official package-manager manifests.
2. `winget` for OS-level developer tools where the manifest is verified and current.
3. npm global installs for Node-based CLIs when that is the official distribution path.
4. rustup for Rust toolchains.
5. `scoop` or `choco` only when they are already installed or clearly superior for the dependency.
6. Manual download/open-browser handoff when authentication, license, or vendor policy requires it.

Every choice must be shown to the operator with source, version, command preview, expected install scope, and rollback/retry notes when feasible.

## CLI Lifecycle

Maestro must be able to manage these CLIs:

- `claude` / Claude CLI.
- `codex` / Codex CLI.
- `gemini` / Gemini CLI.
- `MAESTRO_DEEPSEEK_API_KEY` or `DEEPSEEK_API_KEY` for DeepSeek API peer execution.
- Cloudflare API credentials for primary D1 operations.
- `wrangler` / Cloudflare CLI for D1 fallback and diagnostics.

Wrangler rule:

- Every Wrangler fallback invocation must use `wrangler@latest`.
- Maestro may automatically authorize the Wrangler update/install action when the operator has approved the D1 fallback path, because Cloudflare changes Wrangler frequently.
- The UI must still show the action, source, effective command, and post-update version check.
- If an installed global Wrangler is stale, Maestro should prefer `npx wrangler@latest` or an equivalent official latest invocation over the stale binary.
- Wrangler readiness never replaces Cloudflare API readiness for primary D1 operations.

Lifecycle operations:

- Detect executable path and version.
- Run a headless smoke probe for each agent CLI after installation/authentication.
- Install when missing.
- Update when outdated and authorized.
- Run capability probes.
- Configure model pins or project settings when supported.
- Start authentication flows.
- Pause for browser login, OAuth, device code, MFA, or token entry.
- Verify authenticated status without leaking tokens.
- Record sanitized diagnostics.

## UI Requirements

Maestro must provide a dedicated setup screen:

- Dependency checklist.
- Install plan preview.
- Per-action authorization buttons.
- Background task progress.
- Redacted command output.
- Secure input prompts for tokens/device codes when needed.
- Browser-login handoff buttons.
- Retry/skip/defer controls.
- Final readiness report.

For long installs, the setup screen can be minimized, but tasks remain visible in the activity center and logs.

## Security Requirements

- No secrets in Git-tracked files.
- No tokens in stdout/stderr logs.
- Redact API keys, OAuth tokens, bearer tokens, cookies, and authorization headers before UI or disk persistence.
- Store any local credentials only in ignored runtime files or an encrypted local vault.
- Never require registry writes as part of normal Maestro runtime.
- Never run elevated commands without a clear Windows consent boundary and operator confirmation.
- Prefer per-user installs over machine-wide installs unless the operator explicitly chooses otherwise.

## Administrator Elevation

Maestro may request administrator elevation only for concrete operational needs that cannot be safely completed in user scope.

Examples:

- Machine-wide PATH updates when per-user PATH is insufficient or explicitly chosen.
- PowerShell execution policy changes beyond `Process` or `CurrentUser` scope.
- Installing machine-wide prerequisites when the official installer requires elevation.
- Repairing system-level package-manager configuration.
- Other operator-approved system adjustments required for full Maestro operation.

Elevation rules:

- Show the exact reason elevation is needed.
- Show the action plan and command preview before invoking UAC.
- Prefer per-user PATH and per-user installs first.
- Prefer `ExecutionPolicy -Scope Process` or `CurrentUser` before `LocalMachine`.
- Request elevation per action, not as a blanket app mode.
- Never continue elevated work after the approved action finishes.
- Capture sanitized output and post-action verification.
- Provide rollback or manual reversal instructions when feasible.

PowerShell policy handling must be conservative. Maestro should diagnose the effective policies first, explain which script or tool is blocked, and ask for permission before applying any scoped change.

## Evidence For Support

When setup fails, Maestro should produce a support bundle containing:

- OS version.
- Dependency matrix.
- Package manager availability.
- Commands attempted with secrets redacted.
- Exit codes.
- Sanitized stdout/stderr tail.
- Suggested next action.

This bundle must be easy to attach for Codex/Claude/Gemini analysis.
