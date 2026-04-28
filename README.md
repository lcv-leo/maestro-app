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

Status: functional alpha with live bootstrap, diagnostics, navigation, Cloudflare credential provisioning, AI API credential checks, PostEditor parity, link auditing, and a real background Claude/Codex/Gemini editorial session path.

Current project version: `v0.3.8`.

Maestro is independent from `cross-review-mcp`; it incorporates the same strict convergence discipline in its own application logic. It is designed to run from a folder, keep runtime data out of Git, and store operator protocols, drafts, evidence, and sessions locally under ignored runtime paths.

Target platform: Windows 11+.

Planned modern stack: Tauri 2 + WebView2, React 19, Vite 8, TypeScript 6, Vitest, Biome, ESLint, and lucide-react.

Diagnostic logs are structured NDJSON files under `data/logs/`, one file per app execution, with native/frontend context and per-agent process events so failures can be attached for precise analysis. See `docs/logging.md`.

CLI agents run in background by design, without visible terminal windows in Windows release builds. Real editorial calls do not have an artificial timeout. The operator can choose Claude, Codex, or Gemini to write the first version; that choice is saved with the session while all three remain part of the review cycle. The operator sees friendly progress, elapsed-time heartbeat status, phase status, resume controls, and a selectable UI verbosity level, while raw prompts, stdout, stderr, working drafts, and transcripts stay out of the normal interface and remain protected as ignored local runtime artifacts under `data/sessions/`.

MainSite-bound editing uses a PostEditor parity module, not a generic editor. See `docs/text-editor-decision.md` and `docs/mainsite-compatibility-contract.md`.

First-run dependency checks, authorized background installation, CLI setup, and authentication flows are planned under `docs/runtime-bootstrapper.md`.

CLI adapter feasibility and risks are audited under `docs/cli-agent-audit.md`.

Cloudflare account/token configuration now verifies the token, prepares `maestro_db`, reuses an existing account Secrets Store when present, and creates `maestro` only when no store exists and creation is permitted. Broader API-first D1 publishing remains tracked under `docs/cloudflare-credentials.md`.

Official AI provider API credentials can be saved locally in `data/config/ai-providers.json` and verified against OpenAI, Anthropic, and Gemini model-list endpoints. Full SDK orchestration remains tracked under `docs/ai-provider-credentials.md`, alongside the existing CLI path.

Configuration persistence supports three modes: local JSON for everything, Windows env-var hybrid for tokens/API keys plus JSON for other settings, and Cloudflare remote persistence through D1 `maestro_db` plus Cloudflare Secrets Store. See `docs/configuration-persistence.md`.

The portable ZIP includes `LEIAME.md` with first-run instructions for end users, including `data/config/bootstrap.json`, Cloudflare environment variables, and per-execution NDJSON logs.

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
