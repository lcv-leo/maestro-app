# Changelog

All notable changes to Maestro Editorial AI will be documented in this file.

## [Unreleased]

## [v0.3.8] - 2026-04-28

- Added operator choice for the editorial draft lead. Claude, Codex, or Gemini can now be selected before a session starts; the selected agent is saved with the session, writes the first version, and leads revision fallback order instead of Claude being hardcoded first.
- Isolated editorial CLI child processes inside the session `agent-runs` folder instead of the portable app root, preventing stray files such as root-level `draft.md` from looking like approved final output.
- Clarified the UI and logs around the selected draft lead while preserving the rule that `texto-final.md` is created only after unanimous review approval.
- Expanded `.gitignore` for root-level runtime draft spills and `.tmp/` working directories.
- Enabled automatic release publication from `main`: the release workflow now derives `vX.X.X` from `package.json`, refuses to reuse an existing tag, creates the GitHub Release for the merge commit, and keeps the existing tag/manual release paths.
- Bumped app metadata to `v0.3.8` for the draft-lead and release-automation build.

## [v0.3.7] - 2026-04-27

- Closed the CodeQL `rust/path-injection` findings opened after v0.3.6 by changing the portable app-root regression test to use the current test executable path instead of creating and deleting a dynamic temporary directory.
- Preserved the v0.3.6 runtime startup fix and early crash logger unchanged.
- Replaced the sidebar brand subtitle `Windows 11+ portable` with the current app version label in `vX.X.X` format, sourced from `package.json`.
- Made the API provider settings real: Maestro now loads and saves `data/config/ai-providers.json`, exposes a visible `Salvar APIs` action, and verifies OpenAI, Anthropic, and Gemini credentials against their model-list endpoints without logging raw keys.
- Changed Cloudflare token validation into a provisioning action: `Verificar e preparar` now creates missing `maestro_db`, initializes Maestro tables, reuses an existing account Secrets Store when the plan allows only one, and creates `maestro` only when no store exists and the token has permission.
- Replaced UI-only buttons with real actions for runtime revalidation, agent CLI checks, opening the session minutes file, and link auditing through the native backend.
- Added in-flight editorial diagnostics: each CLI execution now writes a parseable `RUNNING` artifact before launch, logs the child PID after spawn, and records native `session.agent.running` heartbeats with elapsed time plus stdout/stderr byte counters while long agent calls are still active.
- Hardened interrupted-session resume so an unfinished `RUNNING` revision does not become the latest usable draft.
- Hardened CLI startup failure handling so a child process spawned for an editorial agent is killed and reaped if stdin delivery fails before the agent can begin work.
- Sanitized provider API network errors so failed Gemini/OpenAI/Anthropic verification cannot echo request URLs containing API keys.
- Hardened the link-audit public URL gate against private, reserved, documentation, multicast, CGNAT, metadata, IPv6 loopback/link-local/ULA, and IPv4-mapped or IPv4-compatible IPv6 targets while avoiding false positives on public hostnames that merely contain private-looking numbers.

## [v0.3.6] - 2026-04-26

- Fixed a startup crash in the Windows portable build by resolving Maestro's app folder from the running executable instead of the Tauri `BaseDirectory::Executable` resolver, which returned `unknown path` on this environment.
- Added an early native panic/crash logger that writes `data/logs/maestro-crash-*.json` even when the normal NDJSON logger has not completed startup yet.
- Added regression tests for portable app-root resolution and early crash-log writing, then verified the rebuilt release executable stays alive and creates a per-execution NDJSON log.

## [v0.3.5] - 2026-04-26

- Hardened resumable session filesystem scans so session folders and agent artifacts are reconstructed only from validated safe names.
- Replaced recursive markdown/activity discovery with known session artifact accounting to remove CodeQL `rust/path-injection` findings without weakening the resume feature.
- Counted timestamped `protocolo-anterior-*.md` backups through a safe single-level scan and added regression tests for canonical artifact names, dotted session folders, and protocol-backup accounting.

## [v0.3.4] - 2026-04-26

- Reworked the running session status card to use an indeterminate activity meter instead of an artificial completion percentage.
- Changed long-running session time display to `hh:mm:ss` and Brazilian date/time formatting with the Brasilia timezone.
- Humanized visible session, agent, phase, and review labels so the UI no longer exposes internal status codes or development-only wording.
- Added native log-write locking so concurrent agent events cannot interleave and corrupt NDJSON lines.
- Added session resume support: Maestro reads interrupted work under `data/sessions/`, auto-resumes when only one session is available, asks the operator when several sessions exist, and can resume with a newly loaded protocol or the protocol saved in the session.

## [v0.3.3] - 2026-04-26

- Hardened native filesystem access so logs, bootstrap configuration, and editorial session artifacts are validated against a canonical Tauri-resolved app root and confirmed as children of Maestro's local `data/` directory before reads or writes.
- Sanitized editorial `run_id` values into safe path segments before creating session folders, preventing path traversal through session artifact names.
- Updated GitHub release attestation action to `actions/attest-build-provenance@v4.1.0` through the merged Dependabot PR.

## [v0.3.2] - 2026-04-26

- Transferred the public repository from `lcv-leo/maestro-app` to `lcv-ideas-software/maestro-app`.
- Updated README workflow badges, GitHub Pages funding link, site organization links, and release engineering docs for the organization namespace.
- Bumped app/package metadata so the post-migration release verifies GitHub Releases and GHCR publication under the organization owner.

## [v0.3.1] - 2026-04-26

- Fixed Windows child-process spawning so CLI probes, registry env-var reads, `.cmd` shims, PowerShell wrappers, and editorial agents run with `CREATE_NO_WINDOW` and no visible terminal windows.
- Moved long editorial execution onto a native background worker so the Tauri/WebView UI remains responsive while agents run.
- Removed editorial-agent timeouts: real drafting, revision, and review calls may take as long as needed under the inviolable unanimity rule.
- Split startup bootstrap into fast config/env loading plus background dependency preflight so app startup no longer waits for every CLI version probe.
- Added visible no-timeout heartbeat updates while an editorial session is still running, including elapsed time and diagnostic log events.
- Changed non-unanimous or operationally incomplete sessions from final-abort semantics to paused/no-final-delivery semantics.
- Added fallback draft generation across Claude, Codex, and Gemini if the first draft agent fails to produce usable text.
- Added multi-round review/revision behavior: `NOT_READY` review output feeds a new revision round instead of ending the task as failed.
- Updated session minutes and UI language so divergence means continuing/paused work, while `texto-final.md` is still created only after unanimous `READY`.

## [v0.3.0] - 2026-04-26

- Replaced the UI-only CLI smoke path with a real first-pass editorial session command that runs Claude for draft generation and Claude, Codex, and Gemini for review in background.
- Added strict protocol text gating: sessions now block if the full Markdown protocol content was not imported, instead of proceeding with metadata only.
- Added session artifacts under ignored `data/sessions/<run>/`: `prompt.md`, `protocolo.md`, agent run outputs, `ata-da-sessao.md`, and `texto-final.md` only when unanimity is reached.
- Expanded NDJSON diagnostics with frontend runtime context, UI/network/visibility events, console warn/error capture, native panic capture, native log sequence, resolved command paths, and per-agent start/finish records.
- Improved diagnostic redaction so logs keep safe metadata such as token presence, env-var name, source, and scope while continuing to redact raw tokens, API keys, authorization values, cookies, and private material.
- Added live running/completed UI states and agent progress styling so long background operations no longer look static.
- Added Rust coverage for diagnostic redaction rules to protect against future regression.
- Removed previously tracked local AI handoff files from the repository index while preserving them on disk, and expanded `.gitignore` so local AI memory/instruction folders stay out of the public remote.
- Adjusted the Codex CLI adapter away from stdin-only `-` mode to a short prompt argument plus appended stdin payload, avoiding hangs observed in local headless probes.
- Fixed cross-review blockers before release: failed draft generation now stops before reviewer calls, command stdout/stderr are drained concurrently to avoid pipe-buffer stalls, native redaction now matches secret-shaped values inside URLs/JSON/header text, and the UI starts with no protocol loaded until a real Markdown import occurs.

## [v0.2.1] - 2026-04-26

- Fixed Windows CLI preflight detection for npm-style `.cmd` shims and known user install paths so Codex, Gemini, npm, and similar CLIs are not incorrectly shown as missing when they are installed.
- Added real Cloudflare credential validation from the settings screen, including token verification, account reachability, D1 database listing, and Secrets Store reachability without logging raw tokens.
- Fixed Cloudflare token verification for account-owned `cfat_` tokens by using `/accounts/{account_id}/tokens/verify`; user tokens continue to use `/user/tokens/verify`.
- Added Cloudflare env-var scope reporting so the UI distinguishes process, user, and machine environment sources.
- Expanded native, frontend, and CI secret redaction/detection to include Cloudflare account-token and global-key prefixes.
- Updated release engineering policy and end-user instructions to treat beta as a tag suffix (`vX.X.X-betaN`) instead of GitHub prerelease mode.
- Fixed the repository hygiene secret-shape scanner to avoid false positives on normal identifiers such as `cloudflare_persistence_database` while preserving common token/key detection.
- Fixed GitHub Release note generation so Markdown code spans are not treated as shell command substitutions and existing releases receive refreshed notes when the release workflow is rerun.
- Removed GitHub prerelease publishing entirely: beta builds must use explicit `vX.X.X-betaN` tags while still being published as normal GitHub Releases.

## [v0.2.0] - 2026-04-26

- Updated the release artifact upload/download actions to Node 24-capable versions to remove GitHub Actions Node 20 deprecation warnings.
- Expanded the GitHub Release notes template so future releases include download, verification, scope, and current-limit sections.
- Removed the empty Windows console window from release builds by setting the Rust Windows subsystem to `windows`.
- Replaced static/fake session status with a live session monitor that changes state after prompt submission, records visible preflight stages, and explicitly blocks delivery when the real Claude/Codex/Gemini adapters are not connected.
- Made the sidebar navigation functional so session, protocols, evidence, agents, settings, and setup render as separate app sections instead of dumping all configuration panels on the main screen.
- Changed diagnostic logging to create one NDJSON log file per app execution, with a per-run `log_session_id` recorded in each event.
- Added button hover, active, focus, and busy animations so operator actions have immediate visual feedback.
- Added `data/config/bootstrap.json` as the local non-secret pointer file that tells Maestro which persistence backend to use on each app start.
- Added automatic Cloudflare env-var discovery for account ID and API token presence without exposing token values in the UI or logs.
- Added native dependency preflight for Setup so the app reports detected CLI/runtime versions instead of static checklist text.
- Added `LEIAME.md` to the portable release package with first-run configuration, Cloudflare bootstrap, env-var, and logging instructions.

## [v0.1.0] - 2026-04-26

- Initial architecture planning for a portable Windows editorial workbench.
- Added day-zero repository hygiene, CodeQL, CI hygiene checks, Dependabot scaffold, and private protocol ignore policy.
- Added GitHub release engineering plan, governance docs, generated release-note configuration, and Windows 11+ modern stack baseline.
- Added minimal TypeScript project marker so CodeQL has a source tree to analyze before the app scaffold lands.
- Switched CodeQL to GitHub Default Setup only, added modern GitHub Pages artifact deployment, and enabled GitHub Sponsors funding metadata.
- Added the first Tauri/React scaffold and structured NDJSON diagnostic logging under ignored `data/logs/`.
- Added the first background-orchestration UI concept with friendly progress indicators and selectable interface verbosity.
- Added editorial session workflow documentation for prompt intake, full protocol reading, trilateral unanimity, final Markdown, and session minutes.
- Added MainSite compatibility and Cloudflare D1 import/export planning for `bigdata_db.mainsite_posts`.
- Added the integrated editor decision: Maestro uses PostEditor parity over generic TipTap so functionality and final HTML match the existing MainSite editor.
- Added a Maestro-local PostEditor parity module copied from the current `admin-app/MainSite/PostEditor` support surface.
- Added Web Evidence Engine planning for fetch, curl-compatible replay, web search, rendered collection, and human-assisted browser capture for CAPTCHA/login/consent workflows.
- Added ABNT Citation Engine planning and defined Maestro as a deterministic fourth peer that can block final delivery independently of the three AI agents.
- Added Link Integrity Engine planning for link extraction, checking, sanitization, correction proposals, and cross-review escalation.
- Reinforced the inviolable unanimity rule: no final text is delivered while any peer divergence remains, regardless of time, cost, or round count.
- Added Runtime Bootstrapper planning for first-run dependency checks, authorized background installation/update, CLI configuration, and authentication flows inside Maestro UI.
- Added Cloudflare credential settings planning with account ID/API token fields, permission validation, API-first D1 operations, and Wrangler fallback.
- Added official AI provider API/SDK credential planning so Claude, Codex/OpenAI, and Gemini can run through CLI, API, or hybrid transports.
- Added operator-selectable credential storage planning for encrypted vault, Windows user environment variables, or warned local JSON config.
- Established the project versioning and changelog convention as `vX.X.X`.
- Required every Wrangler fallback invocation to use `wrangler@latest`, with automatic update authorization when the fallback CLI path is needed.
- Added a CLI Agent Audit for Codex, Claude, and Gemini, including official documentation links, local headless smoke results, structured-output risks, and adapter hard gates.
- Enabled GitHub Pages first-run provisioning through `configure-pages` and Dependabot Cargo updates now that the Tauri manifest exists.
- Added a tag-driven release workflow that builds a portable Windows ZIP, publishes GitHub Releases, mirrors the release archive to GitHub Packages through GHCR/OCI, and avoids NuGet/installer distribution.
- Triaged the initial Rust Dependabot alerts, constrained Tauri features for the Windows 11+ target, removed unnecessary X11 dependencies from the lockfile, and documented the remaining transitive Tauri risk register.
- Added the three-mode configuration persistence contract: JSON local, Windows env-var hybrid, and Cloudflare remote persistence through D1 `maestro_db` plus Cloudflare Secrets Store.
