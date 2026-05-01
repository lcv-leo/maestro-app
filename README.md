<p align="center">
  <img src=".github/assets/lcv-ideas-software-logo.svg" alt="LCV Ideas &amp; Software" width="520" />
</p>

# Maestro Editorial AI

Portable Windows editorial workbench for protocol-driven AI drafting, source verification, and multi-agent editorial convergence.

[![CI](https://github.com/lcv-ideas-software/maestro-app/actions/workflows/ci.yml/badge.svg)](https://github.com/lcv-ideas-software/maestro-app/actions/workflows/ci.yml)
[![Pages](https://github.com/lcv-ideas-software/maestro-app/actions/workflows/pages.yml/badge.svg)](https://github.com/lcv-ideas-software/maestro-app/actions/workflows/pages.yml)
[![Release](https://github.com/lcv-ideas-software/maestro-app/actions/workflows/release.yml/badge.svg)](https://github.com/lcv-ideas-software/maestro-app/actions/workflows/release.yml)
![CodeQL](https://img.shields.io/badge/CodeQL-default%20setup-enabled-brightgreen)
![status](https://img.shields.io/badge/status-functional%20alpha-blue)
![target](https://img.shields.io/badge/target-Windows%2011%2B-blue)
![stack](https://img.shields.io/badge/stack-Tauri%202%20%2B%20React%2019-blueviolet)
![runtime](https://img.shields.io/badge/runtime-portable-green)
![state](https://img.shields.io/badge/state-JSON%2FNDJSON-informational)
![license](https://img.shields.io/badge/license-AGPL--3.0-blue)

**Status.** Functional alpha with live bootstrap, diagnostics, navigation, Cloudflare credential provisioning, AI API credential checks, PostEditor parity, link auditing, and a real background Claude/Codex/Gemini/DeepSeek editorial session path.

Current project version: `v0.3.17`.

The version history at a glance:

| Release | Scope |
|---|---|
| **`v0.3.17`** | **Code split batch — extracted `src-tauri/src/app_paths.rs`.** Pure refactor (no behavior change) per `docs/code-split-plan.md` migration step 2 ("logging and path safety"). Migrated `APP_ROOT` static + 18 path/safety helpers (`app_root`, `data_dir`, `sessions_dir`, `checked_data_child_path`, `sanitize_path_segment`, etc.) into a self-contained module with doc comments. `initialize_app_root`, `install_process_panic_hook`, and `write_early_crash_record` stayed in `lib.rs` because they touch the Tauri runtime or compose JSON crash records. `lib.rs`: 9759 → 9632 lines. 66 cargo tests still passing; npm typecheck + build clean. |
| **`v0.3.16`** | **Pos-v0.3.15 production-log fixes + Codex NB-2/NB-5 hardenings.** B11 regressao do callsite frontend (resume nao propagava `runOptions`); B13 `send_with_retry` helper aplicado nos 4 providers (1 retry, 1.5s backoff) para PROVIDER_NETWORK_ERROR transitorios; B14 mesma helper respeita `Retry-After` (delta-seconds ou HTTP-date) com default 30s e cap 120s para HTTP 429; B15 contador `consecutive_all_error_rounds` escalava com novo status `ALL_PEERS_FAILING` apos 3 rounds onde todos os peers caem em tone=error. Hardenings: `FinalizeRunningArtifactsGuard` RAII via Drop (cobre panics/early-returns que a hook v0.3.15 nao cobria); novo `clippy.toml` com `disallowed-methods` para `Command::new` + `#![warn(clippy::disallowed_methods)]` em lib.rs (qualquer spawn editorial fora de `hidden_command` agora dispara warn). Clippy hygiene: 2 idiom fixes (`manual_pattern_char_comparison` + `if_same_then_else` no proprio codigo da v0.3.15). 5 testes novos = 66 cargo tests verde. |
| **`v0.3.15`** | **Anti-"casca vazia" sweep — 12 distinct fixes from session-log analysis.** B1 Gemini sandbox trust forced via `GEMINI_CLI_TRUST_WORKSPACE` env; B2 dedicated `AGENT_FAILED_EMPTY` classifier for success+empty review (DeepSeek's frequent 200-OK-empty failure mode); B3 Windows pipe error 109/232/233 classification + diagnostic surfaced in artifact; B4 tail-preserving stderr cap (`truncate_text_head_tail`, head 1 KiB + tail 60 KiB) replaces head-only truncation; B5 aggregator filter routes empty-output statuses through operational failures; B6 `finalize_running_agent_artifacts` rewrites `RUNNING` placeholders at session end (no timeout reintroduced); B7 child-process UTF-8 forced via `PYTHONUTF8`/`LC_ALL`/`LANG`; B8 `#[serde(default)]` on `SessionContract` so legacy contracts no longer fail to parse and overwrite `created_at`; B9 partial — `review_complaint_fingerprint` detects 3-round repeat NOT_READY rebuttals and emits `session.divergence.persistent` warn; B10 UI cap placeholders + tooltips; B11 active_agents wiring (frontend resume now forwards UI selection; backend logs resolution trail via `session.editorial.active_agents_resolved`); B12 cost/time controls audited and confirmed wired (visibility added). Anti-drift refactors: `resolve_effective_active_agents` and `build_active_agents_resolved_log_context` extracted as pure functions so runtime and tests share a single source of decision-tree and NDJSON payload shape. 20 new `#[test]` invariants — 61 cargo tests green. Cross-review-v2 quadrilateral CONVERGED `unanimous_ready` (session `1f259a0e`) after three iterations closed legitimate NEEDS_EVIDENCE / NOT_READY blockers via real code work. |
| **`v0.3.14`** | **Rigorous security/UX audit closure.** Top-level `ErrorBoundary` so render-phase exceptions no longer blank the webview silently; `useEscapeKey` hook wired on the two custom-portal dialogs (`PromptModal`, `ResumeDialog`) that lacked ESC dismissal — mirrors the same fix shipped in admin-app v02.00.00 and mainsite-app v03.22.00. Boundary forwards exceptions to the same `logEvent` NDJSON channel `installGlobalDiagnostics` uses, so the audit trail stays single-source. |
| **`v0.3.13`** | **Session controls, API peers, attachments, and code splitting.** Added selectable peers, optional time/cost caps, UI-managed provider tariffs, real direct-API runners, provider-native attachments, human-readable logs, and lazy PostEditor loading. |
| **`v0.3.12`** | **README organizational standardization.** Adopted the shared repository README opening pattern and added the top-level version-history table while keeping the Windows/Tauri operational details intact. |
| **`v0.3.11`** | **DeepSeek real-peer integration.** Added DeepSeek as an API-backed editorial peer, real model-list verification, and stronger Cloudflare Secrets Store reload behavior. |
| **`v0.3.10`** | **Long-run orchestration reliability.** Fixed broken paused-session rendering, improved visible logs, removed false blocked end states, and reduced very-large prompt pipe failures. |
| **`v0.3.9`** | **Cloudflare persistence and settings maturation.** Continued the secrets/configuration persistence and operational readiness work that led into the current build. |

Maestro is independent from `cross-review-mcp`; it incorporates the same strict convergence discipline in its own application logic. It is designed to run from a folder, keep runtime data out of Git, and store operator protocols, drafts, evidence, and sessions locally under ignored runtime paths.

Target platform: Windows 11+.

Planned modern stack: Tauri 2 + WebView2, React 19, Vite 8, TypeScript 6, Vitest, Biome, ESLint, and lucide-react.

Diagnostic logs are structured NDJSON files under `data/logs/`, one file per app execution, with native/frontend context and per-agent process events so failures can be attached for precise analysis. The app UI shows a human-readable activity summary while the raw NDJSON remains available for deep debugging. See `docs/logging.md`.

CLI agents run in background by design, without visible terminal windows in Windows release builds. DeepSeek runs through the official API path, not a local CLI. Real editorial calls do not have an artificial timeout. The operator can choose Claude, Codex, Gemini, or DeepSeek to write the first version; that choice is saved with the session while all four remain part of the review cycle. The operator sees friendly progress, elapsed-time heartbeat status, phase status, resume controls, and a selectable UI verbosity level, while raw prompts, stdout, stderr, working drafts, and transcripts stay out of the normal interface and remain protected as ignored local runtime artifacts under `data/sessions/`.

MainSite-bound editing uses a PostEditor parity module, not a generic editor. See `docs/text-editor-decision.md` and `docs/mainsite-compatibility-contract.md`.

First-run dependency checks, authorized background installation, CLI setup, and authentication flows are planned under `docs/runtime-bootstrapper.md`.

CLI adapter feasibility and risks are audited under `docs/cli-agent-audit.md`.

Cloudflare account/token configuration now verifies the token, prepares `maestro_db`, reuses an existing account Secrets Store when present, and creates `maestro` only when no store exists and creation is permitted. Broader API-first D1 publishing remains tracked under `docs/cloudflare-credentials.md`.

Official AI provider API credentials can be saved locally in `data/config/ai-providers.json` and verified against OpenAI, Anthropic, Gemini, and DeepSeek model-list endpoints. Full SDK orchestration remains tracked under `docs/ai-provider-credentials.md`, alongside the existing CLI path.

Configuration persistence supports three modes: local JSON for everything, Windows env-var hybrid for tokens/API keys plus JSON for other settings, and Cloudflare remote persistence through D1 `maestro_db` plus Cloudflare Secrets Store. See `docs/configuration-persistence.md`.

The portable ZIP includes `LEIAME.md` with first-run instructions for end users, including `data/config/bootstrap.json`, Cloudflare environment variables, and per-execution NDJSON logs.

The growing native and React surfaces now have a staged modularization plan in `docs/code-split-plan.md`.

Prompt-to-consensus sessions export separate final text and session minutes. Interrupted sessions can be resumed from `data/sessions/`; if a new protocol is loaded before resume, Maestro passes it to the agents and preserves the previous protocol as a local session artifact. See `docs/editorial-session-workflow.md`.

Shared chat import, Markdown/PDF support, and Cloudflare D1 integration are planned under `docs/import-export-cloudflare.md`.

Web fetch, curl-compatible replay, web search, rendered collection, and human-assisted browser capture are planned under `docs/web-evidence-engine.md`.

ABNT citation/reference formatting and Maestro's deterministic fourth-peer role are planned under `docs/abnt-citation-engine.md`.

Link checking, sanitization, correction proposals, and cross-review escalation are planned under `docs/link-integrity-engine.md`.

## Day-Zero Security Posture

- No secrets or API keys in the repository.
- GitHub Secret Scanning, Code Scanning, CodeQL, and Dependabot are assumed active.
- CodeQL uses GitHub Default Setup. Advanced Setup requires prior justification and explicit operator authorization.
- Dependabot alert triage is tracked in `docs/dependabot-alert-triage.md`.
- Operator-supplied editorial protocols are imported through the app and stored locally, not committed.
- Public release requires pre-cloud exposure audit and full-history secret scan.

## Release Planning

GitHub Releases, GitHub Packages, GitHub Pages, and GitHub Sponsors are planned from day zero. Releases publish a portable Windows ZIP; GitHub Packages publishes a GHCR/OCI automation mirror, not NuGet. See `docs/release-engineering-plan.md`.

Version tags, changelog headings, and release labels use the `vX.X.X` format.
