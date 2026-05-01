# Changelog

All notable changes to Maestro Editorial AI will be documented in this file.

## [Unreleased]

## [v0.3.15] - 2026-05-02

### Fixed — session-log-driven anti-"casca vazia" sweep (12 distinct bugs across run-2026-04-26)
The operator analyzed session `run-2026-04-26T19-28-26-698Z` (75 rounds, 34/219 READY, status blocked, infrastructure cascade collapse around round 072) and surfaced ten recurring failure modes. Two more were caught mid-fix: active-peers selection and session caps lacked an end-to-end verification trail (the operator-coined antipattern *casca vazia* — UI exists, background does not actually fire). v0.3.15 closes all twelve at the wiring level, with 12 new `#[test]` invariants proving each fix is functional, not a label change.

- **B1 — Gemini sandbox trust forced via env.** `--skip-trust` was already in `gemini_args()` but failed silently in some operator environments. Centralized `apply_editorial_agent_environment` helper now sets `GEMINI_CLI_TRUST_WORKSPACE=true` when the spawned binary stem is `gemini`, on top of the existing flag. Belt-and-suspenders.
- **B2 — DeepSeek/API empty-output gets a dedicated classifier.** Successful exit (`exit_code == 0`) with `stdout.trim().is_empty()` now produces `AGENT_FAILED_EMPTY` (tone `error`), not the pre-fix `NOT_READY` which masqueraded as a real editorial parecer. Same fix applied to the CLI path (covers all four agents, but DeepSeek is the most frequent victim because its API contract returns 200 OK with empty `choices[0].message.content` under quota/timeout). Tested via `nonzero_empty_review_with_success_exit_classifies_as_agent_failed_empty`.
- **B3 — Windows pipe error 109 surface and classification.** `read_pipe_to_end_counting_classified` returns both the bytes and an optional classification string; `classify_pipe_error` recognizes `windows_error_109_broken_pipe`, `windows_error_232_pipe_closing`, `windows_error_233_pipe_no_listener`, and the kind-only `broken_pipe`/`unexpected_eof`/`interrupted`/`timed_out` shapes. `TimedCommandOutput` carries `stdout_pipe_error` and `stderr_pipe_error`; the editorial agent artifact now includes a `Stdout pipe error` / `Stderr pipe error` diagnostic line when either is present, so what was previously a silent `Err(_) => break` is now operator-visible.
- **B4 — Codex stderr cap is tail-preserving.** `truncate_text_head_tail(value, head_chars, tail_chars)` keeps the head 1 KiB (preamble identifying the command) plus the tail 60 KiB (where the actual error message lives) with a `[... N chars truncated (head 1024 / tail 61440) ...]` marker between. Replaced the pre-fix `sanitize_text(&stderr, 8000)` call which truncated head-only and lost the tail with the actual ConstrainedLanguage / 429 / sandbox details.
- **B5 — Aggregator classification sweep.** `build_blocked_minutes_decision` and `parse_agent_artifact_result` now route `AGENT_FAILED_EMPTY` and `EMPTY_DRAFT` through the operational-failures branch alongside `AGENT_FAILED_NO_OUTPUT` and `EXEC_ERROR_*`. The `tone` derivation in the artifact-result parser also flips both new statuses to `error`. Pre-fix, `EMPTY_DRAFT` got tone `error` but was skipped by the minutes filter; the aggregator under-reported operational failures.
- **B6 — RUNNING perpetual-state finalization.** `finalize_running_agent_artifacts(agent_dir)` is invoked once at the start of `editorial_session_result(...)` (last common path before the result struct is built). It scans `*.md` artifacts in `agent-runs/` and, for any file still containing the `Status: \`RUNNING\`` placeholder, rewrites it to `AGENT_FAILED_NO_OUTPUT` and appends a one-line note explaining the finalization sweep. No timeout is reintroduced (operator's deliberate v0.3.1 design preserved); this is purely state-cleanup at session end. Tested via `finalize_running_agent_artifacts_rewrites_running_to_failed_no_output`.
- **B7 — Pipe-reader UTF-8 forcing.** Verified the protocolo on disk is valid UTF-8 (no BOM, accents `Versão`/`vigência` display correctly via PowerShell `Get-Content -Encoding UTF8`); the corruption observed in past sessions came from child-process pipe encoding under Windows code page 1252. `apply_editorial_agent_environment` now sets `PYTHONIOENCODING=utf-8`, `PYTHONUTF8=1`, `LC_ALL=C.UTF-8`, `LANG=C.UTF-8` for every spawned editorial CLI (Claude, Codex, Gemini, DeepSeek wrapper). Existing `String::from_utf8_lossy` decoding remains the safety net for non-conformant emitters.
- **B8 — Resume preserves `session-contract.created_at`.** Root cause: `SessionContract`'s `links` and `attachments` fields were required `Vec<...>` without `#[serde(default)]`, so any older contract that pre-dated those fields failed to deserialize, `load_session_contract` returned `None`, and `created_at` fell back to `Utc::now()` — overwriting the original session start time. Fix: `#[serde(default)]` on `active_agents`, `initial_agent`, `max_session_cost_usd`, `max_session_minutes`, `links`, `attachments`, plus a new `default_session_contract_schema_version` for `schema_version`. `load_session_contract` now logs parse failures via `eprintln!` instead of swallowing them. Tested via `session_contract_loads_legacy_payload_without_links_attachments`.
- **B9 — Persistent divergence detection (partial).** New `agent_review_fingerprints: BTreeMap<agent_name, Vec<u64>>` carries a per-agent ring buffer of the last three review fingerprints across rounds. `review_complaint_fingerprint(artifact)` extracts the `## Stdout` block, collapses whitespace, and hashes the first 1024 chars to a stable `u64`. When an agent has 3 consecutive identical fingerprints AND status remains non-READY, a `session.divergence.persistent` warn-level NDJSON event is emitted with the agent, round, status, and fingerprint. Marked **partial**: surfaces the deadlock to the operator; full auto-resolution (escalate-to-operator, force-vote, inject-mediator) is a session-contract amendment deferred to a later release. Tested via `review_complaint_fingerprint_stable_across_whitespace_normalization` and `review_complaint_fingerprint_differs_on_distinct_complaints`.
- **B10 — Default session caps placeholders.** UI placeholders changed from generic "ignorar" to concrete suggestions: `60 (em branco = sem teto)` for max minutes, `5.00 (em branco = sem teto)` for max USD, with tooltips explaining that minutes is checked between rounds + per-spawn timeout, and USD only applies to API peers (CLI peers are subscription-billed). Schema still allows null. Pre-existing wiring in `session_time_exhausted` and `provider_cost_guard_for` confirmed not casca-vazia via grep audit (60+ call sites).
- **B11 — `active_agents` selection wiring (newly identified).** Operator reported "fiz o teste com apenas um agente, e o app chamou todos." Root cause: the resume request from `App.tsx` did not forward `active_agents`, so the backend always fell back to the saved contract (which captured all four agents on first start). Fix: frontend now passes `active_agents`, `max_session_cost_usd`, `max_session_minutes`, `attachments`, `links` through the resume invoke alongside the start invoke. Backend writes a new `session.editorial.active_agents_resolved` log entry recording `active_agents_requested`, `active_agents_saved_contract`, `active_agents_effective`, and `active_agents_source ∈ {request, saved_contract, default_all}`, plus the same shape for `max_session_cost_usd_*` and `max_session_minutes_*`. The operator can now audit post-hoc whether the runtime honored the UI selection.
- **B12 — Cost/time controls visibility (operator hypothesis).** The operator suspected these were also casca vazia. Audit confirmed they ARE wired (60+ call sites of `session_time_exhausted` between rounds, `remaining_session_duration` per spawn, `provider_cost_guard_for` before each API spawn, `COST_LIMIT_REACHED` returned and propagated). The fix here is the same `active_agents_resolved` log entry plus B10's UI clarification — both make the runtime decision auditable.

### Diagnostic surface
- New `eprintln!("session_contract_parse_failed path=... error=...")` in `load_session_contract` so silent schema drift never recurs.
- New NDJSON category `session.editorial.active_agents_resolved` records the full resolution trail for active_agents and both caps.
- New NDJSON category `session.divergence.persistent` (warn level) signals 3-round repeat NOT_READY rebuttals per agent.
- Editorial agent artifacts now include `Stdout pipe error` / `Stderr pipe error` lines when pipe reads classified anything other than clean EOF.

### Validation
- `cargo test` — 61 passed, 0 failed (20 new + 41 existing); no flakes.
- `npm run typecheck` — clean.
- `npm run build` — `tsc --noEmit && vite build`, ~1.7s, 2434 modules transformed (pre-existing PostEditor chunk-size warning unchanged).
- Manual verification of B7 on-disk encoding: PowerShell `Get-Content protocolo.md -Encoding UTF8 | Select-String "Versão|vigência"` displays accents correctly, magic bytes confirm no BOM (`23 20 50 72 6F 74 6F 63` = `# Protoc...`).
- **Cross-review-v2 quadrilateral CONVERGED `unanimous_ready` in R1 of session `1f259a0e-00aa-42d2-aec6-5e32278484ab`** with caller=claude and peers=codex+gemini+deepseek (codex emphasis per operator). Two prior cycles (sessions `52f03bd1` and `176ee784`) had legitimate NEEDS_EVIDENCE / NOT_READY blockers that were closed via real code work each time, not by re-asserting: repo-wide spawn-primitive grep confirming `resolved_command_builder` is the only editorial spawn path (closes earlier "lib.rs-only grep" gap), `resolve_effective_active_agents` extracted as a unit-testable helper with 5 direct tests covering request-overrides-saved / saved-fallback / both-missing / empty-saved-recovery / explicit-empty-rejection, then a 6th test for explicit-empty-with-saved (codex/deepseek R1 BL-2), then `build_active_agents_resolved_log_context` extracted as a pure function so the runtime and tests share a single NDJSON payload source (codex/deepseek R1 BL-1) with 2 shape tests pinning all 13 fields plus the three resolution-source variants (`request`/`saved_contract`/`default_all`/`unset`).

### Helper extractions (anti-drift)
- `resolve_effective_active_agents(request: Option<&Vec<String>>, saved: Option<&Vec<String>>) -> Result<(Vec<String>, &'static str), String>` — single source of the resume contract decision tree. Called from `run_editorial_session_inner`. Handles legacy contract recovery: empty saved Vec falls through to default_all instead of erroring (pre-fix would Err).
- `build_active_agents_resolved_log_context(...) -> serde_json::Value` — single source of the `session.editorial.active_agents_resolved` NDJSON payload shape. Called from `run_editorial_session_inner`; both runtime emission and tests consume the same builder, so payload drift is impossible.

## [v0.3.14] - 2026-05-01

### Added — rigorous security/UX audit closure (parity with admin-app v02.00.00 / mainsite-app v02.18.00)
- Top-level `ErrorBoundary` class component (`src/components/ErrorBoundary.tsx`) wired in `main.tsx` around `<App />`. Pre-fix, `installGlobalDiagnostics()` only captured `window.error` and `unhandledrejection` — both fire AFTER React's reconciler. Render-phase exceptions (throw inside JSX, useState selectors, component init) were silently unmounted by React, blanking the webview with no diagnostic trail. The boundary is strictly additive: it forwards captured exceptions to the SAME `logEvent({ level: 'error', ... })` NDJSON channel, so the audit trail stays single-source. React 19 still requires a class component for `componentDidCatch`.
- `useEscapeKey` hook (`src/hooks/useEscapeKey.ts`) — verbatim port from admin-app v02.00.00. Wired in two custom-portal dialogs that lacked ESC dismissal (Radix-built dialogs, `SearchReplacePanel`, and `SlashCommands` already had ESC):
  - `src/editor/posteditor/editor/PromptModal.tsx`: hook called BEFORE the early `return null` to satisfy Rules of Hooks; `enabled = modal.show` keeps the listener detached when hidden. Mirrors the existing Close button (line 43); no new dismissal path.
  - `App.tsx` `ResumeDialog` block (around lines 2588–2640): in-place edit per `docs/code-split-plan.md` ("future splits should start with pure helpers... without mixing large refactors with behavior changes"). Mirrors the existing Close button at the dialog header — same dismissal semantics, no UX-intent change.

### Calibrated out (advisor catch — regression risk > benefit)
- `Promise.race` timeout on direct-API peers — direct-API editorial calls already have explicit per-session deadlines and 2-retry × 800ms structure; a short blanket timeout would regress legitimate long-wait operator flows.
- `EnvSecretsSchema` Zod migration — `readSecretString` + secret-store routing in `lib.rs` is functional; adding Zod is preference, not fix.
- TLS cert pinning on `reqwest` — relies on system trust store via `rustls-tls`; pinning is engineering preference, not a fix in single-operator desktop context.
- Plaintext credential JSON encryption — operator already has the Cloudflare Secrets Store opt-in for keys (v0.3.11); local plaintext is a known design with OS file-permission fallback. Encrypting at rest needs master-password UX, out of scope for this cycle.

### Validation
- `npm run build` — `tsc --noEmit && vite build` — 786 ms, 2434 modules transformed (pre-existing PostEditor chunk-size warning unchanged).
- `cargo check --locked --manifest-path src-tauri/Cargo.toml` — clean (49.65s).

## [v0.3.13] - 2026-05-01

- Added per-session editorial controls: selectable active peers (1-4), optional time limits, optional direct-API cost limits, prompt attachments, and public source links.
- Added real direct-API editorial runners for OpenAI/Codex, Anthropic/Claude, Google/Gemini, and DeepSeek, with API-only mode no longer falling back to CLIs for non-DeepSeek peers.
- Added a UI-managed provider tariff table for per-million token rates. The session still has one optional USD cost limit; provider tariffs calculate/audit observed API usage and any API peer is blocked with a friendly message until its provider input/output rates are configured.
- Added native attachment delivery for supported direct-API providers: OpenAI receives `input_image`/`input_file`, Anthropic receives image/document content blocks, and Gemini receives `inline_data` parts. Unsupported or native-size-exceeding attachment types remain available through manifest and bounded text previews.
- Added pre-run attachment delivery hints in the session UI, showing per active provider whether each file is expected to be sent natively to OpenAI/Anthropic/Gemini or kept as manifest/previews only.
- Added a human-readable log projection under `data/logs/human/` with additive NDJSON fields, concise summaries, and heartbeat collapse so raw structured logs remain machine-readable without forcing operators to read giant JSON lines.
- Persisted session contracts, attachment manifests, link artifacts, and cost ledgers under each ignored `data/sessions/<run>/` folder.
- Started the conservative code-splitting pass: moved human-log and session-control helpers out of `src-tauri/src/lib.rs`, and changed the Tiptap-heavy PostEditor parity surface to load only after `Criar Post`, reducing the production entry chunk from about 1.30 MB to about 272 KB minified.
- Cross-review for the native attachment continuation converged in `cross-review-v2` session `00b642c0-9f7a-4b85-a1cb-f04384548d61` after round 3 with Claude, Gemini, and DeepSeek READY.
- Cross-review for the per-provider attachment delivery UI follow-up converged in `cross-review-v2` session `121cbad3-81fd-4c84-928f-7e30bfdd5d88` after round 2 with Claude, Gemini, and DeepSeek READY.

## [v0.3.12] - 2026-04-30

- `README.md` now follows the shared organizational opening pattern adopted across the public repositories, while preserving Maestro's Windows/Tauri, logging, and editorial-runtime specific documentation.

## [v0.3.11] - 2026-04-28

- Added DeepSeek as a real API-backed editorial peer: it can be selected as draft lead, participates in review/revision rounds, writes normal session artifacts, and selects the best available authenticated DeepSeek `/models` entry, with `deepseek-v4-pro` preferred when exposed.
- Added DeepSeek credential storage and real model-list verification alongside OpenAI, Anthropic, and Gemini.
- Improved Cloudflare Secrets Store reload behavior: Maestro now restores remote secret references from the local marker and `maestro_db` metadata, while clearly treating raw Secret Store values as non-readable by the desktop app.
- Added the initial `docs/code-split-plan.md` roadmap for splitting the growing Rust and React surfaces without mixing refactor work with behavior changes.

## [v0.3.10] - 2026-04-28

- Fixed broken/paused session display overflow by making the app shell viewport-bound, moving page overflow to the workspace, bounding all repeated status lists, and summarizing agent history instead of rendering every historical agent result as visible UI.
- Made visible session logs more human-readable: the UI now shows a concise latest-round summary while the detailed NDJSON and agent artifacts remain technical for diagnosis.
- Changed the editorial orchestrator so review operational failures no longer end the session as paused; they are logged, fed into the next revision attempt, and the cycle continues until unanimous READY.
- Kept sessions running when no revision agent produces a usable draft by retrying the next review round with the current draft instead of returning a blocked result.
- Reduced CLI pipe failures on very large rounds by writing oversized agent prompts to ignored sidecar input files in `data/sessions/<run>/agent-runs/` and sending the CLIs a compact instruction to read the local file.
- Reduced revision-prompt bloat by passing useful review stdout excerpts instead of whole diagnostic artifacts.
- Fixed Cloudflare Secrets Store upsert when a provider secret already exists beyond the first paginated API page, and added a retry path for `secret_name_already_exists`.

## [v0.3.9] - 2026-04-28

- Fixed Cloudflare API credential persistence so `credential_storage_mode=cloudflare` writes AI provider keys to Cloudflare Secrets Store and stores only non-secret markers/metadata locally and in D1.
- Reused the existing Cloudflare Secrets Store when the account already has one, without renaming it, and linked the effective store plus secret references in `maestro_db`.
- Reclassified empty/failed agent review artifacts as operational failures instead of treating them as usable editorial review notes.
- Kept the session activity feed bounded with internal scrolling so long runs no longer stretch the whole app vertically.
- Expanded `ata-da-sessao.md` blocked decisions with concrete operational failures and editorial divergences.

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
