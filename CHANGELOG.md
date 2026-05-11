# Changelog

All notable changes to Maestro Editorial AI will be documented in this file.

## [Unreleased]

## [v0.5.25] - 2026-05-11
### Fixed
- **Autonomous serial reviewer redraw.** The circular scheduler no longer pauses with `PAUSED_SELF_REVIEW_BLOCKED` when the nominal next reviewer is also the current text author. It now autonomously redraws an eligible independent pending reviewer, preserving the hard no-self-review rule while keeping the session moving.
- **Closing-turn guard.** The original redactor cannot receive the closing turn until the independent reviewers for the current version have approved it, so a round cannot shortcut the full editorial circuit.
- **Current-version convergence.** Serial convergence is now based on all independent reviewers approving the current version since the last substantive text change, not on stale per-round flags.
- **Dependabot automerge concurrency.** The automerge workflow now keys concurrency by pull request number, with a `github.ref` fallback for future non-PR triggers, preventing unrelated Dependabot PRs from canceling each other.
### Validation
- `cargo test --locked --lib serial_`: 9 passed.
- `cross-review-v2` session `7075ed2c-8bb0-408c-b622-fe16fbbf9ea1`: recovered unanimity after DeepSeek's evidence follow-up; Claude, Gemini, DeepSeek, and Grok READY.

## [v0.5.24] - 2026-05-10
### Fixed
- **Circular round semantics.** A Maestro round now stays open until the current text completes the full active-agent circuit and returns to the original drafter; individual agent actions are tracked as turns inside the round, so Gemini/DeepSeek/Grok/Codex no longer become separate numbered rounds by themselves.
- **Resume-safe circular custody.** Resumed sessions restore the next circular turn from the latest draft/revision artifact, preventing a resumed run from assigning the current version back to the same agent that just produced it.
- **Strict serial output contract.** Truncated or incomplete `READY` artifacts no longer count as approval or text custody. A valid turn must return a complete `<maestro_revision_report>`, balanced optional `<maestro_final_text>`, explicit `custody`, and no prompt/protocol echo.
- **Protocol echo containment.** Prior revision history no longer injects raw stdout when a revision report is missing; downstream agents receive a compact contract-failure diagnostic instead of duplicated prompt/protocol text.
- **Review token headroom.** Review/rewrite API calls now use the same 20k output-token ceiling as drafting, reducing false convergence and partial artifacts caused by 4k truncation.
- **Release metadata.** App metadata was bumped to `0.5.24` so the release workflow publishes the new padded `v00.05.24` tag instead of skipping the already-published `v00.05.23`.
### Validation
- GitHub code scanning checked before tests: no open alerts returned for `LCV-Ideas-Software/maestro-app`.
- `npm run build`: passed once, with the existing Vite large-chunk warning only.
- `cargo test --manifest-path src-tauri\Cargo.toml --locked --lib`: 153 passed.
- `cargo check --locked --all-targets`, `cargo test --locked --lib`, and `cargo clippy --locked --no-deps --all-targets`: passed after the CI import-scope hotfix.
- `git diff --check`: passed.
- `cross-review-v2` sessions `a549bc27-383c-4fdb-8ca4-a6bba143f949` and `b7f845f4-0c1c-4227-a30f-bd6ace80ea04`: converged.

## [v0.5.23] - 2026-05-10
### Changed
- **Serial editorial deliberation.** Maestro no longer runs the editorial loop as parallel critique plus redactor rewrite. The current text now moves through a serial reviewer-reviser cycle: each independent peer receives the previous peer's text, applies only protocol-grounded corrections, records a revision report, and passes the result onward until every independent reviewer approves the stable text without substantive change.
- **No-self-review hardening.** The current draft/revision author is excluded from the reviewer-reviser panel, and each prompt carries a defensive `SELF_REVIEW_BLOCKED` contract if a bypass ever tries to make an agent revise its own immediately produced text.
- **Approved-content lock.** Reviewer-revisers must preserve already-approved content and may alter only passages tied to concrete prior objections, protocol-blocking defects, or minimal adjacent grammar/continuity fixes. Stable approval now tracks reviewer identities rather than a raw counter, avoiding double-counted convergence.
- **Internal report separation.** Serial revisions write an English internal `<maestro_revision_report>` plus a Brazilian Portuguese `<maestro_final_text>`. Resume recovery accepts only the final-text tag for revision artifacts, preventing internal analysis from leaking into the next public draft.
- **Quality guard for weaker peers.** Lower-tier reviewers such as DeepSeek and Grok are blocked from materially shrinking stronger Claude/Codex text unless the change is protocol-grounded, reducing flattening and loss of argumentative depth.
- **Evidence link audit details.** The evidence panel now lists each invalid, blocked, local/private, malformed, or HTTP-failing link with a specific invalidity reason instead of only reporting a generic invalid-link count.
- **CodeQL test hardening.** The session-orchestration artifact-path test now uses a deterministic crate-local target path instead of a dynamic temp path, addressing the current remote `rust/path-injection` alert pattern pending a GitHub CodeQL rerun after push.

### Validation
- GitHub code scanning checked before tests: 5 open remote CodeQL `rust/path-injection` alerts still present pending this push and a new CodeQL run.
- `cargo test --manifest-path src-tauri\Cargo.toml`: 147 passed.
- `npm run typecheck`: passed.
- `npm run build`: passed once, with the existing Vite large-chunk warning only.
- `git diff --check`: passed.
- `cross-review-v2` session `09c21d7a-008f-48b1-bd48-93d93985cd43`: initial `ship` submission exposed a cross-review-v2 evidence-provenance bug in Grok's relator draft; controlled evidence-backed `ask_peers` recovered unanimity with Claude, Gemini, DeepSeek, and Grok READY, then finalized `converged`.
- `src-tauri/target` verified absent after final validation per workspace directive.

## [v0.5.22] - 2026-05-10
### Changed
- **Recoverable reviewer operational outage.** Review rounds made only of operational reviewer failures no longer flow into text revision or generic "all peers failing" aborts. After repeated independent-reviewer transport/runtime outages, Maestro pauses recoverably as `PAUSED_REVIEWER_OPERATIONAL_OUTAGE`, keeps the current draft, preserves the no-self-review rule, and tells the operator to retry reviewers, switch transport/mode, or enable more independent reviewers.
- **Codex CLI full-prompt delivery.** Codex sidecar runs now receive the complete prompt via stdin while retaining the sidecar artifact for audit, avoiding shell/file-read loops that previously produced empty final output in CLI mode.
- **CLI environment hardening.** Child process PATH resolution now preserves inherited PATH while adding trusted Windows locations such as `C:\npm-global` and WinGet ripgrep paths; peer processes run with deterministic noninteractive UTF-8/CI environment variables.
- **Operational diagnostics.** Maestro now classifies Codex/Gemini empty-output and Gemini ripgrep-missing failures with specific operational statuses, and the UI/minutes explain the paused recoverable reviewer-outage state in user-facing language.

### Validation
- `cargo test --manifest-path src-tauri/Cargo.toml --lib`: 142 passed.
- `npm run typecheck`: passed.
- `npm run build`: passed once, with the existing Vite large-chunk warning only.
- `cross-review-v2` session `c63ae56a-d89f-4dd9-b56c-a3ea5a60064d`: converged READY with independent non-Codex reviewers.
- `src-tauri/target` removed after final validation per workspace directive.

## [v0.5.21] - 2026-05-10
### Changed
- **Resume cycle provenance.** Session contracts now preserve the original initial drafter separately from the agent selected by the operator to resume the current cycle, so choosing a new resume lead no longer rewrites historical authorship.
- **Append-only agent attempts.** Repeated agent calls for the same round/peer/role now write `-attempt-NNN` artifacts instead of overwriting previous attempts, preserving the audit trail used by the session minutes and resume scanner.
- **Operational vs editorial separation.** Operational failures such as no-output peers, upstream CLI failures, provider errors, cost blocks, and operator stops are no longer carried into the next revision prompt as editorial objections.
- **Operational-only review retry.** When a review round produces no concrete editorial blockers, Maestro skips text revision and retries review instead of asking an agent to rewrite already-approved content.
- **CLI stdout cleanup.** Leading Windows process-management noise from peer CLIs is stripped before status parsing and artifact persistence, preventing taskkill output from corrupting Markdown/frontmatter.

## [v0.5.20] - 2026-05-10
### Changed
- **Internal peer language contract.** Draft, review, revision, CLI sidecar, and API system prompts now instruct agents to use en_US for internal coordination while keeping operator-facing draft/revision deliverables in Brazilian Portuguese (pt_BR).
- **Incremental review contract.** Review rounds now distinguish round 1 full audit from later rounds, where peers must focus on unresolved blocking objections and materially new regressions instead of reopening already-approved content.
- **Approved content lock.** Review and revision prompts now treat approved passages as locked content. Later rounds may reopen a passage only when the latest revision changed it, an unresolved `NOT_READY` blocker cites it, or a direct protocol-breaking defect would make final delivery unsafe.
- **Protected approved content rule.** Revision prompts now require minimal, traceable edits: change only passages tied to concrete `NOT_READY` blockers, preserve approved paragraphs/structure/references/claims, reduce broad "read everything again" feedback to specific blockers, and return the current draft unchanged when no concrete blocker authorizes a change.
- **Review objection carry-forward.** The orchestrator now carries prior non-READY review objections into the next review prompt, including resumed sessions, and excludes READY votes from that blocking-objection block.

### Validation
- `cargo test --manifest-path src-tauri\Cargo.toml --locked --lib editorial_prompts -- --nocapture`: 4 passed.
- `cargo test --manifest-path src-tauri\Cargo.toml --locked --lib -- --nocapture`: 131 passed. The two synthetic panic messages are expected tests for Drop semantics.
- `npm run typecheck`: passed.
- `npm run build`: passed once, with the pre-existing Vite large-chunk warning only.
- `git diff --check`: passed, with CRLF normalization warnings only.
- `cross-review-v2` session `1fc08967-7e5b-497e-b2fb-20f71951d9c8`: Claude, Gemini, DeepSeek, and Grok READY in round 1; outcome `converged` / `unanimous_ready`.
- `src-tauri/target` removed after final validation per workspace directive.

## [v0.5.19] - 2026-05-10
### Added
- **Provider prompt-cache policy** for direct API peers without changing model selection, thinking mode, editorial protocol, or unanimity semantics.
- OpenAI/Codex and Grok/xAI direct Responses calls now send deterministic non-secret `prompt_cache_key` values.
- OpenAI sends `prompt_cache_retention: "24h"` only for documented extended-retention model families; unknown future OpenAI model ids keep provider default retention.
- Anthropic/Claude direct Messages calls now send the stable `system` text block with `cache_control: { "type": "ephemeral" }`, leaving variable user prompts unmarked.
- DeepSeek and Gemini keep provider-side automatic/implicit cache behavior without invented request fields.
- Successful API artifacts, `session.agent.finished`, `session.provider.cache.configured`, and per-session `cache-manifest.ndjson` now expose non-secret cache mode, cache-key hash, retention label, stable-prefix size, prompt size, and provider cache token counters when returned.

### Validation
- `cargo test --locked --lib -j 1 cache -- --nocapture`: 10 passed.
- `cargo test --locked --lib -j 1 -- --nocapture`: 130 passed.
- `npm run typecheck`: passed.
- `npm run build`: passed once, with the pre-existing Vite large-chunk warning only.
- `git diff --check`: passed, with CRLF normalization warnings only.
- `cross-review-v2` session `dc5fe326-998a-4bfe-b84e-77f4f5ae725d`: Claude, Gemini, and Grok READY in round 1; outcome `converged` / `unanimous_ready`.
- `src-tauri/target` removed after final validation per workspace directive.

## [v0.5.18] - 2026-05-09
### Alterado
- **`site/index.html`** — iframe `github.com/sponsors/.../card` (caixa branca cross-origin) substituído por link card dark navy com ❤ pink + meta cyan + seta animada; card movido para DEPOIS dos botões (lcv.dev/sponsor primário, GitHub Sponsors alternativa). Companion ship Phase 3 (12 repos). Versão Tauri bumpada em 4 lugares (package.json + tauri.conf.json + Cargo.toml + Cargo.lock).

## [v0.5.17] - 2026-05-09
### Alterado
- **`site/index.html`** — `<style>` block reskinneado pra nova identidade visual dark-first navy/cyan da org LCV (paleta `#050b18`/`#38bdf8`/`#34d399`, gradientes radiais, glow shadows, gradient text no h1). Coordinated companion ship Phase 2 com `calculadora-app` v04.01.17, `oraculo-financeiro` v01.10.04, `astrologo-app` v02.17.23, `admin-app` v02.01.01, `mainsite-app` v03.23.01/v02.19.01, `mtasts-motor` v02.00.10. Companion à Phase 1 (cross-review-v1 1.12.9, cross-review-v2 v02.18.07, deepseek-cli 0.3.1, grok-cli 1.6.2, sponsor-motor APP v01.02.02, `.github-org/site`). Versão Tauri bumpada em `package.json` + `src-tauri/tauri.conf.json` + `src-tauri/Cargo.toml` + `src-tauri/Cargo.lock` (4 lugares). Sem mudança no app desktop; apenas a página GitHub Pages.
- Entrada [Unreleased] anterior (remoção do widget SumUp em `site/index.html`) consolidada aqui — o widget já havia sido removido em ships anteriores.

## [v0.5.16] - 2026-05-06

Grok joins Maestro as the fifth editorial peer.

### Added
- Added Grok / xAI to the frontend provider model: API key field, provider probe row, rate-card fields, active-peer toggle, first-draft author selection, status cards, attachment delivery hints, and Cloudflare storage metadata.
- Added `grok-api` as an API-only backend peer with `MAESTRO_GROK_API_KEY`, `GROK_API_KEY`, and `XAI_API_KEY` credential fallback. Model selection honors `MAESTRO_GROK_MODEL`, `CROSS_REVIEW_GROK_MODEL`, `GROK_MODEL`, or `XAI_MODEL`, then probes xAI `/v1/models` and prefers the strongest Grok 4 lineup available.
- Added direct xAI execution through `https://api.x.ai/v1/responses` with `store: false`, using the same cost preflight, cancellation, usage/cost capture, artifact writing, and status parsing envelope as the other API peers.

### Changed
- Provider mode semantics now cover five peers: **API** runs Claude, Codex, Gemini, DeepSeek, and Grok through official providers; **Hybrid** routes Claude/Codex/Gemini through CLI and DeepSeek/Grok through API; **CLI** disables DeepSeek/Grok instead of pretending a local CLI path exists.
- The backend now blocks any bypassed CLI attempt for DeepSeek or Grok with `API_ONLY_AGENT_DISABLED_IN_CLI_MODE`, so the UI rule is backed by a native guard.
- The tribunal/no-self-review selection logic now normalizes Grok/xAI aliases and keeps the current draft author excluded from the review panel across five peers.

### Validation
- `npm run typecheck`: passed.
- `npm run build`: passed, with the existing Vite large-chunk warning.
- `cargo check --manifest-path src-tauri\Cargo.toml --locked`: passed.
- `cargo test --manifest-path src-tauri\Cargo.toml --locked grok --lib`: 2 passed.
- `cargo test --manifest-path src-tauri\Cargo.toml --locked should_run_agent_via_api --lib`: 4 passed.
- `cargo test --manifest-path src-tauri\Cargo.toml --locked selected_review_agent_specs --lib`: 3 passed.
- `cargo test --manifest-path src-tauri\Cargo.toml --locked`: 120 passed.
- `cargo clippy --manifest-path src-tauri\Cargo.toml --locked --no-deps --all-targets`: passed, with the pre-existing `sanitize.rs` `items_after_test_module` warning.
- `npm audit --audit-level=moderate`: found 0 vulnerabilities.
- `git diff --check`: passed, with CRLF normalization warning on `src/App.tsx`.
- Real xAI smoke: `GET https://api.x.ai/v1/models` returned 16 models and `POST https://api.x.ai/v1/responses` returned `response_ok: true` with selected model `grok-4.3`.
- `cross-review-v2` session `5f4c2d44-898c-4267-9821-a72d1e1be14c`: caller Codex, reviewers Claude/Gemini/DeepSeek/Grok, round 1 unanimous READY / `unanimous_ready`.

## [v0.5.15] - 2026-05-05

Hardening of Maestro's judicial-collegiate editorial model.

### Fixed
- Review panel selection now fails closed when the current draft/revision author cannot be verified. A resumed session with an unreadable or unparseable current-author artifact now pauses with `PAUSED_DRAFT_AUTHOR_UNKNOWN` instead of risking self-review.
- The backend now uses the same canonical no-self-review rule for all review panel selection: the current text author is the petitioner for that cycle and cannot vote as reviewer of the same text.
- The fail-closed diagnostic policy is logged as `fail_closed_no_self_review_without_known_petitioner` under `session.tribunal.draft_author_unknown` so incident response can grep for the exact tribunal-rule block.

### Changed
- Draft, review, and revision prompts now explicitly describe Maestro's tribunal-style cycle: the redactor is the petitioner, reviewers are an independent editorial panel, votes are `READY`/`NOT_READY`, and revisions are new deliberative cycles inside the same append-only case file.
- User-facing status labels now render `PAUSED_DRAFT_AUTHOR_UNKNOWN` as "Autor do rascunho nao identificado".

### Validation
- `npm run typecheck`
- `npm run build` (pre-existing Vite large-chunk warning only)
- `cargo test --manifest-path src-tauri\Cargo.toml --locked independent_review_agent_specs --lib` (3 passed)
- `cargo test --manifest-path src-tauri\Cargo.toml --locked` (118 passed)
- `cargo clippy --manifest-path src-tauri\Cargo.toml --locked --no-deps --all-targets` (pre-existing `sanitize.rs` `items_after_test_module` warning only)
- `git diff --check` (CRLF normalization warnings on existing config files only)
- `cross-review-v2` session `b02655ca-cd23-4361-b25b-f3f673ce1ce0`: Claude, Gemini, and DeepSeek unanimous READY. Grok was excluded from the clean quorum because the runtime reported an xAI provider-auth error unrelated to this delta.

## [v0.5.14] - 2026-05-03

Hotfix for resumed sessions whose permanent session `run_id` matched old cost-ledger history.

### Fixed
- Resumed executions now create a fresh `cost_scope_id` per orchestration attempt. The session `run_id` remains the stable folder/session id, while cost-limit enforcement and new ledger entries use the attempt scope.
- Legacy ledgers whose top-level `run_id` is the same as the resumed session id no longer charge their historical entries against a newly requested resume cost limit.
- The `session.editorial.active_agents_resolved` NDJSON event now includes `cost_scope_id`, making future budget investigations distinguishable from the permanent session id.
- Resumed sessions now have explicit tests pinning author recovery from draft/revision artifacts, so the current author remains excluded from reviewing their own text. This formalizes Maestro's colegiate-review rule: the agent that drafts or resumes the current text is the petitioner for that cycle, not a voting reviewer of that same text.

### Validation
- `cargo test --manifest-path src-tauri/Cargo.toml --locked cost_ledger_ --lib`: 3 passed.
- `cargo test --manifest-path src-tauri/Cargo.toml --locked selected_review_agent_specs --lib`: 3 passed.
- `cargo test --manifest-path src-tauri/Cargo.toml --locked can_agent_review_current_draft --lib`: 1 passed.
- `cargo test --manifest-path src-tauri/Cargo.toml --locked resume_author_recovery --lib`: 2 passed.
- `npm run typecheck`: passed.
- `npm run build`: passed, with the existing Vite large-chunk warning.
- `npm audit --audit-level=moderate`: found 0 vulnerabilities.
- `cargo check --manifest-path src-tauri/Cargo.toml --locked`: passed.
- `cargo test --manifest-path src-tauri/Cargo.toml --locked`: 112 passed.
- `cargo clippy --manifest-path src-tauri/Cargo.toml --locked --no-deps --all-targets`: passed with the pre-existing `items_after_test_module` warning in `sanitize.rs`.
- `cross-review-v2` session `a7f8e77a-ba3b-4d2d-82b5-472529563b62`: converged in round 4 with Claude, Gemini, and DeepSeek READY after literal code/test evidence was supplied.

## [v0.5.13] - 2026-05-03

Hotfix for resumed sessions inheriting previous cost-ledger totals.

### Fixed
- `cost-ledger.json` now scopes `total_observed_cost_usd` to the current `run_id` when a session is resumed. Historical entries remain in the ledger, but previous runs no longer consume the new resume attempt's cost ceiling.
- New cost entries persist their `run_id`; legacy entries without `run_id` are attributed to the ledger's saved `run_id` during load so old history remains auditable without blocking the current run.
- Rootless legacy ledgers without a top-level `run_id` now deserialize safely and keep their entries as `__legacy_unscoped__` history instead of counting them against the resumed attempt.

### Validation
- `cargo test --manifest-path src-tauri/Cargo.toml --locked cost_ledger_ --lib`: 2 passed.
- `npm run typecheck`: passed.
- `npm run build`: passed, with the existing Vite large-chunk warning.
- `npm audit --audit-level=moderate`: found 0 vulnerabilities.
- `cargo check --manifest-path src-tauri/Cargo.toml --locked`: passed.
- `cargo test --manifest-path src-tauri/Cargo.toml --locked`: 106 passed.
- `cargo clippy --manifest-path src-tauri/Cargo.toml --locked --no-deps --all-targets`: passed with the pre-existing `items_after_test_module` warning in `sanitize.rs`.
- `git diff --check`: passed; Git still warns that `src-tauri/tauri.conf.json` CRLF will normalize to LF when touched.
- `cross-review-v2` session `7b63c0da-140f-4083-b8d3-bad4b76ff1a3`: converged in round 2 with Claude, Gemini, and DeepSeek READY after adding top-level `CostLedger.run_id` serde compatibility and the rootless legacy-ledger regression test.

## [v0.5.12] - 2026-05-03

Production-log follow-up for DeepSeek artifacts and API spend controls.

### Fixed
- DeepSeek API responses now persist only the final assistant `message.content` as agent stdout. Responses that contain only `reasoning_content`, empty content, or `finish_reason=length` are recorded as provider failures with concise diagnostics; raw provider JSON is no longer treated as an editorial draft/review artifact.
- DeepSeek API cancellation now uses tone `blocked` with status `STOPPED_BY_USER`, matching the CLI cancellation path instead of surfacing an operator stop as an error.
- Sessions with API-backed peers now require an explicit USD session cost limit before any paid provider call is started. The frontend blocks start/resume with a user-facing warning, and the backend returns `PAUSED_COST_LIMIT_REQUIRED` as a hard safety gate if a caller bypasses the UI.

### Validation
- `npm run typecheck`: passed.
- `npm run build`: passed, with the existing Vite large-chunk warning.
- `cargo check --manifest-path src-tauri/Cargo.toml --locked`: passed.
- `cargo test --manifest-path src-tauri/Cargo.toml --locked deepseek_ --lib`: 6 passed.
- `cargo test --manifest-path src-tauri/Cargo.toml --locked`: 105 passed.
- `git diff --check`: passed; Git still warns that `src/App.tsx` CRLF will normalize to LF when touched.
- `cross-review-v2` session `d84e5aa7-24fd-460d-97b3-f55226e10f10`: round 3 unanimous READY; outcome `converged` / `unanimous_ready`.

## [v0.5.11] - 2026-05-03

Production-log hotfix for v0.5.10 session behavior and diagnostics.

### Fixed
- Review rounds now exclude the current draft/revision author. An agent can no longer review its own draft, including resumed sessions where the current author is recovered from the latest `round-NNN-...-{draft|revision}.md` artifact.
- Sessions with no independent reviewer left after that exclusion now pause explicitly as `PAUSED_REVIEWERS_UNAVAILABLE`, with a user-facing "Sem revisor independente" status and guidance to select at least two active agents before resuming.
- Cost-capped API review rounds now preflight the projected cost of the whole independent review round before starting. If the remaining budget cannot cover all API reviewers, Maestro pauses before launching a partial review round.
- Sanitized JSON logs now preserve diagnostic field names such as `cloudflare_api_token_env_scope` while still redacting secret values. Field names are no longer corrupted into redacted fragments.
- Resume-start telemetry no longer reports misleading `prompt_chars: 0` or fake protocol metadata when the prompt/protocol are loaded from the saved session rather than the current frontend editor.
- Blocked-session frontend logs now persist only the agent count and latest 12 agent summaries instead of embedding the full growing agent history in one NDJSON event.
- Test fixtures no longer contain literal token-shaped placeholders such as `cfat_...`; redaction tests now assemble those values at runtime to avoid false-positive Secret Scanning alerts.

### Validation
- `npm run typecheck`: passed.
- `npm run build`: passed, with the existing Vite large-chunk warning.
- `cargo check --manifest-path src-tauri/Cargo.toml --locked`: passed.
- `cargo test --manifest-path src-tauri/Cargo.toml --locked`: 101 passed.
- `git diff --check`: passed; Git still warns that `src/App.tsx` CRLF will normalize to LF when touched.
- There is no `npm test` script in `package.json`; frontend validation remains `typecheck` + production `build` for this patch.
- Cross-review-v2 session `4f6cb687-2bd1-4d8a-af56-cc7aa0e76b49`: recovered unanimity in round 3; Claude and Gemini READY in round 2, DeepSeek READY after the requested literal diff/test evidence in round 3.

## [v0.5.10] - 2026-05-03

Artifact correctness hotfix from production-session evidence. `ata-da-sessao.md` now groups agent entries by the real `round-NNN-...` artifact name instead of placing every entry under a hard-coded `Rodada 001`. `texto-final.md` is now a clean public deliverable: the internal first-line `MAESTRO_STATUS: READY|NOT_READY` protocol marker is stripped when writing the final text after unanimity, while agent artifacts keep the raw marker for audit/debugging.

### Operational policy
- New release-close directive: after a finalized Maestro version is delivered, delete `src-tauri/target` from the local workspace to keep `C:\Users\leona\lcv-workspace` lean. This cleanup is post-validation only and must resolve the path under `maestro-app\src-tauri\target` before deletion.

### Validation
- Focused Rust tests added for round grouping, unparseable-round visibility, final-text marker stripping, and UTF-8 BOM/CRLF marker handling.
- `cargo test --locked strip_leading_maestro_status --lib`: 2 passed.
- `cargo test --locked build_session_minutes_groups_agents_by_real_artifact_round --lib`: 1 passed.
- `cargo test --locked --lib`: 98 passed.
- Cross-review-v2 session `113c453f-57fc-477b-a647-6e52116949a5`: caller Codex + Claude/Gemini/DeepSeek converged READY in round 1; two non-blocking hardening follow-ups were applied before final validation.

## [v0.5.9] - 2026-05-02

Pure refactor batch — frontend split. Extracted three new modules from [src/App.tsx](src/App.tsx) per `docs/code-split-plan.md` (frontend track). No behavior change. Per advisor's explicit guidance, scope is **pure data only**: types, constants, pure helpers. NO sub-component extraction. NO hook refactor.

### Extracted from App.tsx (3 modules, ~849 lines moved)
**[src/types.ts](src/types.ts)** (~253 lines, 41 type exports): All type aliases (lines 37-279 of v0.5.8 App.tsx) — `ProtocolSnapshot`, `AgentState`, `VerbosityMode`, `PhaseState`, `ProviderMode`, `AiCredentialKey`, `InitialAgentKey`, `ProviderRateKey`, `NativeAttachmentProvider`, `CredentialStorageMode`, `CloudflareTokenSource`, `ActiveSection`, `SettingsTab`, `RunStatus`, `ActivityLevel`, `NavItem`, `OperationSnapshot`, `AgentCard`, `ActivityItem`, `PhaseItem`, `DiscussionRound`, `EvidenceRow`, `CloudflarePermissionRow`, `BootstrapCheckRow`, `BootstrapConfig`, `CloudflareEnvSnapshot`, `DependencyPreflight`, `CloudflareProbeResult`, `CloudflareProviderStorageRequest`, `AiProviderConfig`, `AiProviderProbeRow`, `AiProviderProbeResult`, `LinkAuditRow`, `LinkAuditResult`, `EditorialAgentResult`, `EditorialSessionResult`, `PromptAttachmentPayload`, `AttachmentDeliveryPlan`, `SessionRunOptions`, `ResumableSessionInfo`, `ProtocolReadingGate`. Imports `ComponentType` from React for `NavItem.icon`.

**[src/constants.ts](src/constants.ts)** (~287 lines, 25 const exports): All initial seed values + static option tables (lines 281-523 of v0.5.8 App.tsx) — `initialAgents`, `initialEvidenceRows`, `initialProtocolReadingGates`, `initialDiscussionRounds`, `finalArtifacts`, `importChannels`, `contentPipelines`, `webEvidenceTools`, `initialBootstrapChecks`, `initialCloudflarePermissionChecks`, `initialAiProviderChecks`, `credentialStorageModes`, `storageModeSummaries`, `aiProviderRows`, `providerRateRows`, `initialAgentOptions`, `defaultActiveAgents`, `attachmentLimits`, `verbosityOptions`, `navGroups`, `navItems`, `settingsTabs`, `idleOperation`, `idlePhases`, `idleActivityFeed`. Imports lucide-react icons used by these tables (Bot, Database, Eye, EyeOff, FileText, GitBranch, Globe2, HardDriveDownload, KeyRound, ListChecks, Settings).

**[src/helpers.tsx](src/helpers.tsx)** (~309 lines, 27 function exports): All pure helper functions (lines 525-809 of v0.5.8 App.tsx) — `stateLabel`, `stateIcon` (returns JSX, hence the .tsx extension), `sha256`, `formatElapsedTime`, `formatBytes`, `normalizedAttachmentMediaType`, `attachmentExtension`, `isTextLikeAttachment`, `isImageAttachment`, `isPdfAttachment`, `isKnownDocumentAttachment`, `providerSupportsNativeAttachment`, `attachmentDeliveryPlan`, `providerShortLabel`, `attachmentDeliveryHint`, `formatBrazilDateTime`, `humanizeRunStatus`, `operationMeterLabel`, `humanizeAgentStatus`, `humanizeRole`, `agentStateFromTone`, `agentResultRank`, `latestAgentResults`, `latestAgentCards`, `latestProtocolGateItems`, `countAgentRounds`, `summarizeAgentResults`. Imports types from `./types` and `attachmentLimits` from `./constants`, plus 4 lucide icons (AlertTriangle, CheckCircle2, Clock3, RefreshCw) used by `stateIcon`.

### Visibility delta
The only modification to the moved code is `type X` → `export type X`, `const Y` → `export const Y`, `function Z` → `export function Z` (TypeScript's equivalent of the Rust `pub(crate)` visibility upgrade in earlier batches). All function bodies, type structures, constant initializers, and JSX are preserved verbatim.

### Out of scope (deferred — explicit operator-validated boundaries)
- Sub-component extraction (no `<SettingsPanel />`, `<SessionHeader />`, etc.). The 3000-line App() component stays intact; this batch is data-only.
- Hook refactor or useEffect/useCallback rewrites. The 28 preexisting `aria-label` linter warnings and 4 hook-deps warnings (`react-hooks/exhaustive-deps` complaints) in the App() body are NOT addressed here — they predate v0.5.9 and the build pipeline (`tsc --noEmit && vite build`) passes clean without them.
- Service module extraction (no separate `services/` folder for invoke wrappers). Future batch.

### App.tsx trim
`src/App.tsx` lucide-react import block trimmed: removed `Eye`, `Settings`, `GitBranch` (no longer referenced after constants.ts move). Removed `ComponentType` from `'react'` import (only used by the moved `NavItem` type, which now imports it inside types.ts). Type imports trimmed to drop names not actually referenced in App.tsx body (`EditorialAgentResult`, `NativeAttachmentProvider`, `RunStatus`).

### Validation
- `npm run build`: clean. `tsc --noEmit` succeeds; vite production bundle generates dist/ with same chunk shape as v0.5.8 (chunk-jRWAZmH_.js + index + lib + PostEditor).
- App.tsx: 3775 → 3077 lines (−698 net).
- types.ts: 253 lines new.
- constants.ts: 287 lines new.
- helpers.tsx: 309 lines new.
- Total moved: 849 lines (lib weight stays comparable, just split across modules).

### Versioning
Patch bump (v0.5.8 → v0.5.9) — pure refactor, no signature/dep/feature changes for end users.

## [v0.5.8] - 2026-05-02

Pure refactor batch — extracted [src-tauri/src/session_orchestration.rs](src-tauri/src/session_orchestration.rs) (~1004 lines incl. doc header) per `docs/code-split-plan.md`. **Largest single split since v0.4.0.** No behavior change.

### Extracted from lib.rs (2 functions)
- `run_editorial_session_inner` (thin wrapper, ~7 lines).
- `run_editorial_session_core` (the orchestration loop with cancel propagation, FinalizeRunningArtifactsGuard, between-rounds checks, contract resolution — ~925 lines).

### Re-export shim in lib.rs
```rust
pub(crate) use crate::session_orchestration::{
    run_editorial_session_core, run_editorial_session_inner,
};
```
session_commands.rs continues to import these via `use crate::{run_editorial_session_core, run_editorial_session_inner, ...};` and resolves through the shim.

### Visibility upgrade in lib.rs
`checked_data_child_path` and `sanitize_path_segment` (from `crate::app_paths::*`) promoted from plain `use` to `pub(crate) use` so sibling modules (`session_evidence.rs`) that already referenced them via `crate::checked_data_child_path` continue to resolve. `sessions_dir` similarly `#[cfg(test)] pub(crate) use` for the existing `#[cfg(test)] use crate::sessions_dir;` in `session_evidence.rs::tests`.

### Cleanup in lib.rs (massive)
After moving the orchestration body, the following imports in lib.rs became unused (consumed only inside session_orchestration.rs):
- `std::collections::{BTreeMap, BTreeSet}`, `std::path::Path`, `std::thread`, `std::fs::self` (collapsed to `std::fs`); kept only `PathBuf, Output, Duration`. `fs`/`Path`/`thread` re-imported `#[cfg(test)]`-gated for the existing `mod tests`.
- `crate::app_paths::sessions_dir` (test-only now)
- `crate::editorial_agent_runners::run_editorial_agent_for_spec` (removed)
- `crate::editorial_helpers::{filter_existing_agents_to_active_set, resolve_effective_active_agents, review_complaint_fingerprint, FinalizeRunningArtifactsGuard}` → `#[cfg(test)]`-only
- `crate::editorial_inputs::{build_active_agents_resolved_log_context, resolve_time_budget_anchor}` → `#[cfg(test)]`-only
- `crate::editorial_prompts::{build_draft_prompt, build_review_prompt, build_revision_prompt}` removed; kept `editorial_agent_specs, ordered_editorial_agent_specs, resolve_initial_agent_key`
- `crate::session_minutes::build_session_minutes` removed
- `crate::session_persistence::{append_agent_cost_to_ledger, load_cost_ledger, load_session_contract, write_session_contract}` → `#[cfg(test)]` for the two used by tests; the other two removed
- `crate::session_resume::{parse_created_at, remaining_session_duration, session_time_exhausted}` removed (orchestration uses its own direct imports)
- `crate::session_controls::{effective_draft_lead, provider_cost_guard_for, sanitize_optional_positive_f64, sanitize_optional_positive_u64, selected_editorial_agent_specs}` removed
- `crate::session_evidence::process_session_evidence` removed
- `pub(crate) use crate::provider_config::*` trimmed: dropped `api_provider_for_agent`, `provider_cost_rates_from_config`, `should_run_agent_via_api` (kept `sanitize_ai_provider_config` used by tests via `super::*`)
- `pub(crate) use crate::editorial_io::*` trimmed: dropped `editorial_session_result` and `SessionResultContext` (consumed only inside session_orchestration.rs)

### Out of scope (v0.5.9)
Frontend `src/App.tsx` (~3775 lines) split — types + constants + pure helpers extraction. Sub-component extraction explicitly deferred.

### Validation
- `cargo test --locked --lib`: **93 passed** (zero regressions vs v0.5.7).
- `cargo clippy --locked --no-deps --all-targets`: **0 lib + 0 test warnings**.
- `npm run build`: clean.
- Function-body byte-parity diff vs v0.5.7 (commit `7a6a451`, lib.rs lines 612-1544 vs new session_orchestration.rs lines 72-1004): **clean** (zero diff after stripping the trailing brace boundary).

### Versioning
Patch bump (v0.5.7 → v0.5.8) — pure refactor, no signature/dep/feature changes.

### File metrics
- lib.rs: 3072 → 2127 lines (−945 net).
- session_orchestration.rs: 1004 lines new.

## [v0.5.7] - 2026-05-02

Pure refactor batch — extracted [src-tauri/src/session_commands.rs](src-tauri/src/session_commands.rs) (4 Tauri commands + 3 blocking workers, ~449 lines incl. doc header) per `docs/code-split-plan.md`. No behavior change. Largest single split since v0.5.6.

### Extracted from lib.rs (7 items)
**Tauri commands (4)**: `list_resumable_sessions`, `resume_editorial_session`, `run_editorial_session`, `stop_editorial_session`.

**Blocking workers (3)**: `run_editorial_session_blocking`, `resume_editorial_session_blocking`, `list_resumable_sessions_blocking`.

### Visibility upgrades in lib.rs
- `ResumeSessionRequest` struct + 10 fields: private → `pub(crate)`.
- `EditorialSessionResult` struct + 17 fields: private → `pub(crate)`.
- `run_editorial_session_inner` fn: `fn` → `pub(crate) fn` (called from session_commands.rs).
- `run_editorial_session_core` fn: `fn` → `pub(crate) fn` (called from session_commands.rs for resume).

The 4 Tauri commands also gained `pub(crate)` so the re-export shim in lib.rs feeds them into `tauri::generate_handler!`.

### Re-export shim in lib.rs
```rust
use crate::session_commands::{
    list_resumable_sessions, resume_editorial_session, run_editorial_session,
    stop_editorial_session,
};
```
Tauri's `generate_handler!` macro in `pub fn run()` resolves the 4 command identifiers from this `use` statement.

### Cleanup in lib.rs
The following imports were used only by the 3 extracted `*_blocking` helpers and are now removed from lib.rs (consumed only inside `session_commands.rs`):
- `crate::app_paths::safe_run_id_from_entry` — moved to `#[cfg(test)]` (only tests use it now)
- `crate::session_artifacts::{inspect_resumable_session_dir, load_resume_session_state}` — moved to `#[cfg(test)]`
- `crate::session_resume::{extract_saved_initial_agent, extract_saved_prompt, extract_saved_session_name, stable_text_fingerprint}` — first three moved to `#[cfg(test)]`; `stable_text_fingerprint` removed entirely (only used in resume blocking)

### Out of scope (deferred to v0.5.8)
`run_editorial_session_inner` and `run_editorial_session_core` (~954 lines of orchestration with cancel propagation, FinalizeRunningArtifactsGuard, between-rounds checks, contract resolution) stay in lib.rs for v0.5.7. Will move to `session_orchestration.rs` in v0.5.8.

### Validation
- `cargo test --locked --lib`: **93 passed** (zero regressions vs v0.5.6).
- `cargo clippy --locked --no-deps --all-targets`: **0 lib + 0 test warnings** (maintained baseline).
- `npm run build`: clean.
- Function-body byte-parity diff vs v0.5.6 (commit `e477ba3`): only the 4 Tauri commands gained `pub(crate)` visibility; all NDJSON shapes, log categories, B22 comment block, RAII cancel guard, and resume contract resolution preserved verbatim.

### Versioning
Patch bump (v0.5.6 → v0.5.7) — pure refactor, no signature/dep/feature changes.

### File metrics
- lib.rs: 3460 → 3072 lines (−388 net).
- session_commands.rs: 449 lines new.

## [v0.5.6] - 2026-05-02

Pure refactor batch — extracted [src-tauri/src/tauri_commands.rs](src-tauri/src/tauri_commands.rs) (11 Tauri commands, ~341 lines incl. doc header) per `docs/code-split-plan.md`. No behavior change. Largest single split batch since v0.4.0.

### Extracted from lib.rs (11 #[tauri::command] items)
**Config CRUD (4)**: `read_bootstrap_config`, `write_bootstrap_config`, `read_ai_provider_config`, `write_ai_provider_config`.

**Diagnostics + observability (3)**: `runtime_profile`, `write_log_event`, `diagnostics_snapshot`.

**Editorial utilities (4)**: `verify_ai_provider_credentials`, `audit_links`, `open_data_file`, `run_cli_adapter_smoke`.

### Visibility upgrade
`RuntimeProfile` struct in lib.rs upgraded from private to `pub(crate)` (with all 7 fields `pub(crate)`) for cross-module access.

### Re-export shim in lib.rs
```rust
use crate::tauri_commands::{
    audit_links, diagnostics_snapshot, open_data_file, read_ai_provider_config,
    read_bootstrap_config, run_cli_adapter_smoke, runtime_profile, verify_ai_provider_credentials,
    write_ai_provider_config, write_bootstrap_config, write_log_event,
};
```
Tauri's `generate_handler!` macro in `pub fn run()` resolves the 11 command identifiers from this `use` statement.

### Cleanup in lib.rs (massive)
The following imports were used only by the 11 extracted commands and are now removed from lib.rs (consumed only inside `tauri_commands.rs`):
- `serde_json::Value` (gated `#[cfg(test)]` — only test uses remain)
- `crate::ai_probes::run_ai_provider_probe`
- `crate::app_paths::{ai_provider_config_path, bootstrap_config_path, data_dir}`
- `crate::cli_adapter::{cli_adapter_specs, run_cli_adapter_probe}`
- `crate::config_persistence::{enrich_ai_provider_config_from_cloudflare, persist_ai_provider_cloudflare_marker, persist_ai_provider_config, persist_ai_provider_config_to_cloudflare, persist_bootstrap_config}` re-export removed
- `crate::link_audit::run_link_audit`
- `crate::logging::LogWriteResult`
- `crate::provider_config::{merge_ai_provider_env_values, normalize_cloudflare_token_source, normalize_storage_mode}` re-export removed (still pub(crate) in provider_config.rs for direct imports)

`persist_ai_provider_cloudflare_marker` re-imported `#[cfg(test)]`-gated for the existing test in `lib.rs::tests`.

### Out of scope (deferred to next batches)
Session orchestration commands (`run_editorial_session`, `resume_editorial_session`, `list_resumable_sessions`, `stop_editorial_session`) stay in lib.rs because they are tightly coupled with the `*_blocking` helpers and `run_editorial_session_core` (~880 lines of orchestration logic still in lib.rs).

### Validation
- `cargo test --locked --lib`: **93 passed** (zero regressions vs v0.5.5).
- `cargo clippy --locked --no-deps --all-targets`: **0 lib + 0 test warnings** (maintained baseline).
- `npm run build`: clean.
- Function-body byte-parity diff vs v0.5.5 (commit 0744639): clean.
- lib.rs: 3729 → 3460 lines (−269 net). tauri_commands.rs: 341 lines new.

### Cross-review pré-Commit & Sync
Cross-review-v2 quadrilateral pendente (HARD GATE 2026-04-26).

### Versioning
Patch bump (v0.5.5 → v0.5.6) — pure refactor.

## [v0.5.5] - 2026-05-02

Pure refactor batch — extracted [src-tauri/src/cloudflare_commands.rs](src-tauri/src/cloudflare_commands.rs) (~153 lines incl. doc header) per `docs/code-split-plan.md`. No behavior change.

### Extracted from lib.rs
- `cloudflare_env_snapshot` — Tauri command probing process env for `MAESTRO_CLOUDFLARE_ACCOUNT_ID` / `CLOUDFLARE_ACCOUNT_ID` / `CF_ACCOUNT_ID` and the matching API_TOKEN family. Returns scope (process / HKCU / HKLM) for each detected variable.
- `dependency_preflight` (async) + `dependency_preflight_inner` (private) — Settings panel CLI/version checks for Claude / Codex / Gemini / Node / npm / cargo / gh plus Cloudflare env state + Wrangler hint. Async wrapper uses `spawn_blocking` to keep IPC thread free.
- `verify_cloudflare_credentials` — Tauri command wrapping `cloudflare::run_cloudflare_probe` with `settings.cloudflare.verify_completed` NDJSON log emission.

### Visibility upgrade
`CloudflareEnvSnapshot` struct in lib.rs upgraded from private to `pub(crate)` (with all 6 fields `pub(crate)`) for cross-module access. `CloudflareProbeRequest` and `CloudflareProbeResult` were already `pub(crate)`.

### Re-export shim in lib.rs
```rust
use crate::cloudflare_commands::{
    cloudflare_env_snapshot, dependency_preflight, verify_cloudflare_credentials,
};
```
Tauri's `generate_handler!` macro in `pub fn run()` resolves the 3 command identifiers from this `use` statement.

### Cleanup in lib.rs
- `crate::cloudflare::{run_cloudflare_probe, token_source_label}` import removed (now consumed only inside cloudflare_commands.rs).
- `crate::command_spawn::command_check` import removed (now consumed only inside cloudflare_commands.rs::dependency_preflight_inner).

### Validation
- `cargo test --locked --lib`: **93 passed** (zero regressions vs v0.5.4).
- `cargo clippy --locked --no-deps --all-targets`: **0 lib + 0 test warnings** (maintained v0.5.3+ baseline).
- `npm run build`: clean.
- Function-body byte-parity diff vs v0.5.4 (commit 812d988): clean.
- lib.rs: 3847 → 3729 lines (−118 net). cloudflare_commands.rs: 153 lines new.

### Cross-review pré-Commit & Sync
Cross-review-v2 quadrilateral pendente (HARD GATE 2026-04-26).

### Versioning
Patch bump (v0.5.4 → v0.5.5) — pure refactor.

## [v0.5.4] - 2026-05-02

Pure refactor batch — extracted [src-tauri/src/editorial_io.rs](src-tauri/src/editorial_io.rs) (10 items, ~256 lines incl. doc header) per `docs/code-split-plan.md`. No behavior change.

### Extracted from lib.rs
- **File I/O**: `write_text_file`, `read_text_file` — both sandboxed via `app_paths::checked_data_child_path`.
- **Path helper**: `command_working_dir_for_output` — derives spawn working directory from per-agent output path with `app_root` fallback.
- **Result builder**: `editorial_session_result` + `SessionResultContext<'a>` (now `pub(crate)`) — assembles `EditorialSessionResult`, runs `finalize_running_agent_artifacts` v0.3.16 NB-2 guard.
- **NDJSON loggers**: `log_editorial_agent_finished` / `log_editorial_agent_spawned` / `log_editorial_agent_running` — emit `session.agent.finished/spawned/running` schema_version=2 events.
- **Output parsers**: `extract_maestro_status` (parses MAESTRO_STATUS contract from stdout), `extract_stdout_block` (extracts fenced ## Stdout body from agent artifacts), `api_error_message` (best-effort error message from JSON HTTP-error bodies).

### Re-export shim in lib.rs
```rust
pub(crate) use crate::editorial_io::{
    api_error_message, command_working_dir_for_output, editorial_session_result,
    extract_maestro_status, extract_stdout_block, log_editorial_agent_finished,
    log_editorial_agent_running, log_editorial_agent_spawned, read_text_file,
    write_text_file, SessionResultContext,
};
```
Preserves all 10 unqualified call sites in `provider_runners.rs`, `provider_deepseek.rs`, `editorial_agent_runners.rs`, `command_spawn.rs`, and the 10 SessionResultContext call sites in `run_editorial_session_core`.

### Cleanup in lib.rs
- `crate::command_spawn::CommandProgressContext` import removed (now used only inside editorial_io.rs).
- `crate::editorial_helpers::finalize_running_agent_artifacts` import gated `#[cfg(test)]` (the only remaining lib.rs caller is the test in `tests::finalize_running_agent_artifacts_rewrites_running_to_failed_no_output`).

### Validation
- `cargo test --locked --lib`: **93 passed** (zero regressions vs v0.5.3).
- `cargo clippy --locked --no-deps --all-targets`: **0 lib + 0 test warnings** (same as v0.5.3 baseline).
- `npm run build`: clean.
- Function-body byte-parity diff vs v0.5.3 (commit 116154f): clean.
- lib.rs: 4054 → 3847 lines (−207 net). editorial_io.rs: 256 lines new.

### Cross-review pré-Commit & Sync
Cross-review-v2 quadrilateral pendente (HARD GATE 2026-04-26).

### Versioning
Patch bump (v0.5.3 → v0.5.4) — pure refactor, no signature/dep/feature changes (workspace `version-control.md` patch criterion: "ajustes menores").

## [v0.5.3] - 2026-05-02

Operator-driven hardening pass: D (proactive sweep) + B subset (items_after_test_module fix + CLI cancel artifact status refinement + duplicate `#[allow]` cleanup).

### (D) Proactive `or_else(saved)` sweep — completed clean
After 4 saved-contract leak bugs in sequence (v0.3.38 provider mode, v0.3.42 caps, v0.5.1 peers, v0.5.2 initial_agent + caps in resume), grep'd `lib.rs` for any remaining `or_else` patterns where saved fallbacks could silently override request values. **All 5 remaining `or_else` patterns are intentional and content-not-config**: `prompt` (extract_saved_prompt fallback to saved_prompt body), `session_name` (fallback to "Sessao {run_id}" placeholder), `protocol_text` (override fallback to saved protocol body), `effective_initial_agent` (already fixed in v0.5.2 B22 with correct request-first priority), `links` (already retained intentionally as content). Pattern fully consolidated. No additional fixes needed.

### (B) `items_after_test_module` fix
[src-tauri/src/lib.rs](src-tauri/src/lib.rs) `pub fn run()` (88 lines, the Tauri 2 binary entry with `#[cfg_attr(mobile, tauri::mobile_entry_point)]`) moved from after `#[cfg(test)] mod tests {}` (line ~3967) to before it (now line ~2541, just above mod tests). Resolves Gemini's last clippy warning. Function body byte-identical; only position changed. The `mod tests` block remains the very last item in the file, matching Rust convention.

### (B) CLI cancel artifact status refinement
[src-tauri/src/editorial_agent_runners.rs](src-tauri/src/editorial_agent_runners.rs) `run_editorial_agent` now detects the operator-cancel branch explicitly: `let stopped_by_user = result.timed_out && cancel_token.is_cancelled();`. When true, the agent artifact is classified with status `STOPPED_BY_USER` (tone `blocked`) instead of routing through the generic `EMPTY_DRAFT`/`AGENT_FAILED_NO_OUTPUT` path. Differentiates real session-deadline timeouts (cancel_token never fired) from operator-driven stops (cancel_token cancelled, then poll loop killed the child via `kill_process_tree`). Closes deepseek's R1 follow-up #1 from v0.5.0 cross-review. New artifact note explains the operator-stop cause and the resume path. Aligns CLI cancel artifact-level status with API cancel artifact-level status (already STOPPED_BY_USER explicit since v0.5.0).

### Cleanup — duplicate `#[allow]`
[src-tauri/src/editorial_agent_runners.rs:169-170](src-tauri/src/editorial_agent_runners.rs#L169) had two consecutive `#[allow(clippy::too_many_arguments)]` attributes on `run_editorial_agent` (introduced inadvertently during the v0.5.0 cancel_token plumbing). Removed the duplicate.

### Validation
- `cargo test --locked --lib`: **93 passed** (no test changes; existing classify_upstream_cli_failure_* tests cover the upstream-bug branches; no new tests for STOPPED_BY_USER classification at agent level since the logic is a 1-line `&&` check).
- `cargo clippy --locked --no-deps --all-targets`: **0 lib warnings + 0 test warnings**. The `items_after_test_module` warning is resolved; the `duplicated attribute` warning is resolved.
- `npm run build`: clean.

### Cross-review pré-Commit & Sync
Cross-review-v2 quadrilateral pendente (HARD GATE 2026-04-26).

### Versioning
Patch bump (v0.5.2 → v0.5.3) — 3 small focused changes (sweep audit + 2 bugfix/hygiene); no signature/dep/feature changes.

## [v0.5.2] - 2026-05-02

Behavior fix — B22: resume path was carrying TWO additional saved-contract fields forward over operator's request.

### Operator report
"Eu não defini tempo nem custos na última sessão, mas ele encerrou o trabalho com o tempo e custos definidos em outra sessão." Operator clicked Retomar at 17:06 BRT (2026-05-02T20:06:57Z), did NOT enter cost or time caps in the UI, but the session was killed with `COST_LIMIT_REACHED` after burning 4.91 USD against a silently-applied 5.0 USD cap from a prior session. Same session also silently inherited 10-min cap.

### Empirical evidence (operator's log `maestro-2026-05-02T20-06-37Z-pid13460.ndjson`)
- Frontend `session.editorial.requested` event correctly logs: `max_session_cost_usd: None, max_session_minutes: None` (operator's UI had no caps).
- Backend `session.editorial.active_agents_resolved` event: `max_session_cost_usd_requested: 5.0, max_session_cost_usd_source: "request"`. **Source mislabeled** — the value WAS substituted from saved_contract but appeared in the request struct AFTER mutation, so the resolver saw it as "request".
- Frontend also sent `requested_initial_agent: codex` but the resolver's `invalid_initial_agent` field showed `deepseek` — the saved value won.

### Root cause — TWO separate bugs in [src-tauri/src/lib.rs:resume_editorial_session_blocking](src-tauri/src/lib.rs)

**(1) Initial_agent priority inverted (line 1160-1162 pre-fix):**
```rust
let effective_initial_agent = saved_initial_agent
    .clone()
    .or_else(|| requested_initial_agent.clone());
```
The `saved_initial_agent` (extracted from saved prompt.md by `extract_saved_initial_agent`) wins over `requested_initial_agent` (operator's UI choice). Reversed semantics from the v0.3.15 B11 / v0.3.42 B20 / v0.5.1 B21 pattern.

**(2) Caps `or_else(saved)` fallback (line 1248-1257 pre-fix):**
```rust
max_session_cost_usd: request.max_session_cost_usd.or_else(|| {
    saved_contract.as_ref().and_then(|contract| contract.max_session_cost_usd)
}),
max_session_minutes: request.max_session_minutes.or_else(|| {
    saved_contract.as_ref().and_then(|contract| contract.max_session_minutes)
}),
```
This is the EXACT pattern that v0.3.42 B20 fix removed in `run_editorial_session_blocking` — but the resume path (`resume_editorial_session_blocking`) was overlooked. The v0.3.42 fix description says "saved caps no longer silently re-applied on resume", but the resume code path itself was not patched.

### Fix
- Flipped initial_agent priority: `requested_initial_agent.or_else(saved_initial_agent)` so operator's UI wins.
- Removed `.or_else(saved...)` for `max_session_cost_usd` and `max_session_minutes` and `active_agents` (consistency). Resume path now passes operator's request values through unmodified.
- Kept `links` `.or_else(saved.links)` because links is a content field (not a config cap) — operator typically doesn't re-enter links on resume; carrying forward saved links is the correct UX.

### What stays unchanged
- Frontend already sends correct request values (per v0.5.1 B21 + earlier).
- Backend `resolve_effective_active_agents` resolver unchanged (already correct).
- Backend NDJSON `session.editorial.active_agents_resolved` log unchanged.
- `links` saved fallback preserved.

### Pattern (final consolidation)
**For all CONFIG fields (active_agents, initial_agent, max_session_cost_usd, max_session_minutes), in BOTH start AND resume paths: request is source of truth; saved is reference only.** Mirrors v0.3.42 B20 (caps in start path) + v0.5.1 B21 (peers in frontend). With v0.5.2, all three layers (frontend, run path, resume path) are aligned.

### Validation
- `cargo test --locked --lib`: **93 passed**.
- `cargo check`: clean.
- `npm run build`: clean.

### Cross-review pré-Commit & Sync
Cross-review-v2 quadrilateral pendente (HARD GATE 2026-04-26).

### Versioning
Patch bump (v0.5.1 → v0.5.2) — pure bugfix, no signature/dep/feature changes.

## [v0.5.1] - 2026-05-02

Behavior fix — B21: resume MUST honor operator's current React state, NOT silently override with saved session contract.

### Operator report
"maestro-app também importa os peers anteriormente configurados, não respeitando novas configurações." — operator changed the peer selection in the UI, clicked "Retomar", and the session ran with the saved-contract peers instead of the new selection.

### Root cause
[src/App.tsx:1660-1679](src/App.tsx) `startResumeSession` called `setActiveAgents(validSavedAgents)` and `setInitialAgent(resolvedInitial)` AT RESUME TIME, overriding whatever the operator had in the React UI. Then built `resumeRunOptions = { activeAgents: validSavedAgents }` which the backend correctly honored as `_source: "request"` — but the "request" was constructed from saved values, not from the operator's current selection.

This was the v0.3.18 B17 fix's behavior ("auto-pre-populate from saved on resume so cold-open + click Retomar continues with same peers"), which solved one scenario but broke the more common scenario of operator deliberately changing peer config before resume.

### Fix (App.tsx:startResumeSession)
- Removed `setActiveAgents(validSavedAgents)` and `setInitialAgent(resolvedInitial)` overrides.
- Removed the if-validSavedAgents branch entirely.
- Always use `currentSessionRunOptions()` (which reads React state).
- The `saved_active_agents` / `saved_initial_agent` fields stay in `ResumableSessionInfo` for the picker UI to display informationally (operator can SEE what the session was running with) but are NEVER auto-applied.
- New NDJSON event `session.resume.contract_applied` log now records both `saved_*` (informational) and `requested_*` (what's actually used) so audit trail proves request semantics.

This mirrors the v0.3.42 B20 fix for cost/time caps: **request is source of truth; saved_contract is reference only**. Both peer selection and caps now follow the same semantics.

### What stays unchanged
- Backend `resolve_effective_active_agents` already correctly prefers request over saved (v0.3.15 B11 + tests).
- Backend `session.editorial.active_agents_resolved` NDJSON log still surfaces `_source ∈ {request, saved_contract, default_all}` so auditors can verify semantics.
- Saved fields in `ResumableSessionInfo` remain available for future picker UI enhancement (showing operator "this session was running with [claude, codex]" in the resume dialog) — but no auto-apply.

### Validation
- `cargo test --locked --lib`: **93 passed** (no test changes; existing `resolve_effective_active_agents_request_overrides_saved` covers the backend invariant that the fix relies on).
- `npm run build`: clean.

### Cross-review pré-Commit & Sync
Cross-review-v2 quadrilateral pendente (HARD GATE 2026-04-26).

### Versioning
Patch bump (v0.5.0 → v0.5.1) — pure bugfix, no signature/dep/feature changes.

## [v0.5.0] - 2026-05-02

Operator-driven session stop. The single largest UX gap pre-v0.5.0 was that the only way to abort a long editorial session was killing the maestro-app process. Real operator log `data/sessions/run-2026-05-02T11-39-41-113Z` shows a 35-minute session with peers running 3-7 minutes each, ending with `session.agent.running` Codex still chugging at 13:49 — the operator killed the app. v0.5.0 ships full async stop with sub-2-second cancel granularity.

### New "Parar sessão" button — frontend
- [src/App.tsx](src/App.tsx) header button visible only when `isRunPreparing === true && sessionRunId != null`. Clicking pops a confirmation dialog, then invokes `stop_editorial_session` IPC. Disabled after click until session ends. Uses lucide `Square` icon.
- New React state `isStopRequested` resets via `try { ... } finally { setIsStopRequested(false); }` in `runRealEditorialSession` (covers success, error, and stop paths uniformly).

### New backend infrastructure
- **`src-tauri/src/session_cancel.rs`** (~140 lines): `static SESSION_CANCEL: OnceLock<Mutex<HashMap<String, CancellationToken>>>` registry keyed by `run_id`. Functions: `register_session_cancel`, `signal_session_cancel`, `unregister_session_cancel`, `CancelTokenGuard` (RAII Drop). 5 unit tests cover idempotency, panic-safe Drop, and unknown-run_id graceful return.
- **New Tauri command `stop_editorial_session(run_id)`** — sync, returns immediately. Logs `session.user.stop_requested` NDJSON event. Registered alongside `run_editorial_session` and `resume_editorial_session` in the Tauri `invoke_handler`.
- **`run_editorial_session_blocking` + `resume_editorial_session_blocking`** register a token at start, build a `CancelTokenGuard`, pass `&CancellationToken` down through `run_editorial_session_inner` → `run_editorial_session_core` → `run_editorial_agent_for_spec` → `run_provider_api_agent`.
- **Between-rounds cancel check** in `run_editorial_session_core` at the top of every round loop iteration. Emits `session.user.stop_completed` warn NDJSON event and returns the session result with status `STOPPED_BY_USER`. Existing `FinalizeRunningArtifactsGuard` (Drop semantics from v0.3.16) preserves agent-runs/* artifacts so the operator can resume the session later from the same run_id.

### CLI peer cancel — `command_spawn::run_resolved_command_observed`
New optional `cancel_token: Option<&CancellationToken>` parameter. The 250ms poll loop now also checks `token.is_cancelled()`; when fired, invokes `kill_process_tree(&mut child)` (Windows: `taskkill /T /F /PID <id>`) and returns the partial output as `timed_out: true`. Cancel granularity ≤500ms. The runner classifies the truncated output as `STOPPED_BY_USER` artifact via existing `editorial_agent_runners.rs` flow.

### Async API peer cancel — `provider_retry::send_with_retry_async`
- New async sibling to `send_with_retry` (sync version removed since it has no remaining callers — all 4 runners migrated). Same retry policy (1 network retry + 1 Retry-After-respecting 429 retry, capped at 120s).
- Wraps `request.send().await` in `tokio::select! { biased; _ = cancel_token.cancelled() => Cancelled, r = future => r }` so an in-flight HTTP request is dropped via reqwest's future-cancellation. Cancel granularity <2s (bounded by the time it takes the runtime to drop the future + close the TCP connection).
- Cancel-aware sleeps in retry backoffs (`tokio::time::sleep` wrapped in `tokio::select!` against cancel).
- New return type: `Result<reqwest::Response, ProviderRequestOutcome>` with `Cancelled` and `Network(reqwest::Error)` variants.
- New `build_api_client_async(timeout)` builds `reqwest::Client` (async) with the Maestro user-agent.

### 4 runners migrated to async — provider_runners.rs + provider_deepseek.rs
- `run_openai_api_agent`, `run_anthropic_api_agent`, `run_gemini_api_agent`, `run_deepseek_api_agent` are now `pub(crate) async fn` taking `(request: EditorialAgentRequest<'_>, cancel_token: &CancellationToken)`.
- Each runner builds 2 clients: blocking for the short-lived `/models` resolve probe (sync), async for the main editorial request (cancel-aware).
- `tokio::select!` wraps `response.text().await` so a long body-read is also cancellable.
- On `Cancelled`: writes `STOPPED_BY_USER` artifact via existing `write_provider_failure_result` (helpers/`write_deepseek_error_result`) so the artifact-discovery downstream code sees a normal-shape blocked result.

### Orchestration dispatch — editorial_agent_runners.rs
- `run_provider_api_agent` uses `tauri::async_runtime::block_on` to bridge the sync session loop into each async runner. Editorial session runs in a `tauri::async_runtime::spawn_blocking` worker (lib.rs:1002), so block_on from there is safe (creates a current-thread runtime, no nested-runtime risk).
- `run_editorial_agent_for_spec` accepts `&CancellationToken` and passes through to either the API-agent dispatch or `run_editorial_agent` (CLI path).

### Dependencies
- New: `tokio = { version = "1", default-features = false, features = ["macros", "time", "sync", "rt"] }`
- New: `tokio-util = { version = "0.7", default-features = false }` (provides `CancellationToken`)
- `reqwest` features unchanged: `["blocking", "json", "rustls-tls"]`. Both `reqwest::Client` (async) and `reqwest::blocking::Client` are compiled in.

### Tests
- 5 new unit tests in `session_cancel::tests` covering: signal-unknown-id returns false, register/signal/unregister round-trip, signal idempotent after first cancel, `CancelTokenGuard` unregisters on Drop, guard Drop runs on panic.
- Existing 88 tests unchanged. Total: **93 passed, 0 failed**.
- Integration test for "HTTP abort under 2s" deferred — would require mock server + tokio runtime in test harness; planned for v0.5.1 follow-up if operator hits a regression.

### Validation
- `cargo test --locked --lib`: **93 passed** (88 + 5 new).
- `cargo clippy --locked --no-deps --all-targets`: **1 lib warning + 2 test warnings**. Same shape as v0.4.0 (only `items_after_test_module` cosmetic remains).
- `npm run build`: clean.

### Operator workflow
1. Press "Parar sessão" → confirmation dialog → confirm.
2. Backend signals cancellation token immediately (Tauri command returns in milliseconds).
3. In-flight CLI peer killed in ≤250ms via `kill_process_tree`.
4. In-flight API peer dropped in <2s via `tokio::select!`.
5. Session loop exits at next round boundary with status `STOPPED_BY_USER`.
6. Artifacts in `data/sessions/<run-id>/agent-runs/` preserved.
7. Operator can later use the standard "Retomar" flow to continue from the same `run_id`.

### Cross-review pré-Commit & Sync
Cross-review-v2 quadrilateral pendente (HARD GATE 2026-04-26).

### Versioning
Minor bump (v0.4.0 → v0.5.0) per workspace `version-control.md`: "Minor: Novas funcionalidades, melhorias significativas". New user-facing feature (stop button) + new IPC surface (`stop_editorial_session`) + dependency additions (tokio, tokio-util) + significant async migration. NOT a breaking change for users — existing flows (start, resume, complete) are unchanged.

## [v0.4.0] - 2026-05-02

Architectural refactor closing Gemini's `too_many_arguments` × 7 finding from the rigorous audit on HEAD `4b56e0d` (v0.3.47). Two new `pub(crate)` context structs collapse 9-11 positional parameters into 1-2 grouped arguments. No behavior change.

### New `ProviderInvocation<'a>` struct (provider_runners.rs)
Groups the 7 parameters that flowed through every editorial helper:
```rust
#[derive(Clone, Copy)]
pub(crate) struct ProviderInvocation<'a> {
    pub log_session: &'a LogSession,
    pub run_id: &'a str,
    pub name: &'a str,           // "Codex", "Claude", "Gemini", "DeepSeek"
    pub cli: &'a str,            // "openai-api", "anthropic-api", etc.
    pub provider: &'a str,       // "openai", "anthropic", "gemini", "deepseek"
    pub role: &'a str,
    pub output_path: &'a Path,
}
```
`Clone, Copy` is free because all fields are references or `&str`. Built once inside each runner from the runner's hardcoded `name`/`cli`/`provider` constants plus the request's spawn context.

### New `EditorialAgentRequest<'a>` struct (provider_runners.rs)
Groups the 9 parameters that `run_*_api_agent` runners receive from the dispatch site in `editorial_agent_runners.rs`:
```rust
pub(crate) struct EditorialAgentRequest<'a> {
    pub log_session: &'a LogSession,
    pub run_id: &'a str,
    pub role: &'a str,
    pub prompt: String,
    pub attachments: &'a [AttachmentManifestEntry],
    pub output_path: &'a Path,
    pub timeout: Option<Duration>,
    pub config: &'a AiProviderConfig,
    pub cost_guard: Option<ProviderCostGuard>,
}
```
Passed by-value so each runner takes ownership of `prompt: String` and `cost_guard: Option<ProviderCostGuard>`. The DeepSeek runner ignores `attachments` because chat-completions does not natively accept inline payloads (matches existing v0.3.x behavior).

### 4 helpers refactored — `provider_runners.rs`
| Function | Before | After |
|---|---|---|
| `api_cost_preflight_result` | 11 args | 5 args |
| `write_provider_missing_key_result` | 9 args | 3 args |
| `write_provider_error_result` | 10 args | 4 args |
| `write_provider_failure_result` | 13 args (had `#[allow]`) | 7 args (no `#[allow]` needed) |

Each helper now takes `&ProviderInvocation` as its first parameter; internal references swap from `name`/`cli`/`provider`/`role`/`output_path`/`log_session`/`run_id` to `invocation.name`/`invocation.cli`/etc. Function bodies otherwise unchanged.

### 4 runners refactored — `provider_runners.rs` + `provider_deepseek.rs`
| Function | Before | After |
|---|---|---|
| `run_openai_api_agent` | 9 args | 1 arg (`EditorialAgentRequest`) |
| `run_anthropic_api_agent` | 9 args | 1 arg |
| `run_gemini_api_agent` | 9 args | 1 arg |
| `run_deepseek_api_agent` | 8 args | 1 arg |

Each runner destructures the request at the top, builds `let invocation = ProviderInvocation { ... };` once with its hardcoded `name`/`cli`/`provider`, and passes `&invocation` to every helper call. Body otherwise byte-identical to v0.3.48.

### Call site updated — `editorial_agent_runners.rs:run_provider_api_agent`
Single dispatch point: builds `EditorialAgentRequest` once at the top, then `match spec.key` moves the request into the matching runner. The fallback `_ => write_provider_error_result(...)` constructs an inline `ProviderInvocation` for the "API_PROVIDER_NOT_SUPPORTED" branch with `provider: "unknown"` and `cli: api_cli_for_agent(spec.key)`.

### Out of scope
- `write_provider_success_result` (17 args, retains `#[allow]`) — would need a third "outcome" struct (`ProviderRunOutcome`) to fit; deferred to keep v0.4.0 focused.
- `log_provider_api_started` (11 args, retains `#[allow]`) — same rationale.
- `write_deepseek_error_result` (7 args, exactly at threshold) — no clippy warning, structurally distinct from the unified family.
- `items_after_test_module` in lib.rs:2542 — last remaining cosmetic warning; will resolve naturally as `pub fn run()` is moved into a future split.

### Tests + Gates
- `cargo test --locked --lib`: **88 passed** (zero regressions vs v0.3.48). All existing coverage exercises the refactored paths via runner dispatch tests.
- `cargo clippy --locked --no-deps --all-targets`: **0 lib warnings + 1 test warning** (down from 7 + 8 in v0.3.48). The only remaining warning is `items_after_test_module`. **All 7 `too_many_arguments` warnings eliminated** — the refactor closes Gemini's primary finding from the rigorous audit.
- `npm run build`: clean.

### Versioning
Minor bump (v0.3.48 → v0.4.0) per the workspace's `version-control.md`: "Minor: Novas funcionalidades, melhorias significativas". Architectural refactor that resolves a deferred audit finding qualifies as melhoria significativa even though no user-facing behavior changed. NOT a breaking change for end users.

### Cross-review pré-Commit & Sync
Cross-review-v2 quadrilateral pendente (HARD GATE 2026-04-26).

## [v0.3.48] - 2026-05-02

Pure refactor batch continuing `docs/code-split-plan.md` migration step 2 ("logging and path safety" tail items deferred from v0.3.17). No behavior change.

### Extracted to `src-tauri/src/app_init.rs` (~140 lines incl. doc header, 5 items)
- `initialize_app_root(app: &tauri::App) -> Result<(), String>` — invoked from `pub fn run()` Tauri `setup` hook; resolves portable root via `app_paths::resolve_portable_app_root` and stores it in the `OnceLock` via `try_set_app_root`.
- `install_process_panic_hook()` — installs `std::panic::set_hook` that forwards every native panic to `write_early_crash_record` so crash trail exists even if normal NDJSON logger has not finished startup.
- `write_early_crash_record(payload, location)` — writes `data/logs/maestro-crash-<timestamp>-pid<pid>.json` with payload + location + app/process metadata. Cap-limited via `sanitize_text` (1000 char payload, 500 char location). Schema unchanged from v0.3.47 (`schema_version: 1`, `category: native.panic`, `level: fatal`).
- `hidden_command(program)` — the SAFE-FUNNEL `Command::new` allowed by `clippy.toml` `disallowed-methods`. Always passes through `apply_hidden_window_policy`.
- `apply_hidden_window_policy(command)` (module-private) — Windows-only `creation_flags(CREATE_NO_WINDOW = 0x08000000)`; no-op on non-Windows. So editorial peer spawns never flash a console window.

### Re-export shim in lib.rs
`pub(crate) use crate::app_init::{hidden_command, initialize_app_root, install_process_panic_hook};` preserves all unqualified call sites in `pub fn run()` and sibling modules. `write_early_crash_record` is re-imported `#[cfg(test)]`-gated for the existing `tests::writes_early_crash_record_before_normal_logger` invariant.

### Cleanup in lib.rs
- `app_paths` import block trimmed: `active_or_early_logs_dir`, `app_root_if_initialized`, `resolve_portable_app_root`, `try_set_app_root` removed (now consumed inside `app_init.rs`); `active_or_early_logs_dir` re-imported `#[cfg(test)]`-gated for the same crash-record test.
- `std::process::{self, Command, Output}` → `std::process::Output` (process and Command now consumed only inside `app_init.rs`).

### What stays in lib.rs
- `pub fn run()` — Tauri 2 binary entry point with `#[cfg_attr(mobile, tauri::mobile_entry_point)]` attribute, which prefers to live in lib.rs.

### Validation
- `cargo test --locked --lib`: **88 passed** (zero regressions vs v0.3.47).
- `cargo clippy --locked --no-deps --all-targets`: **7 lib + 8 test warnings**, same shape as v0.3.47 (no new warnings; remaining 7 are deferred architectural set: `too_many_arguments` × 7 + `items_after_test_module` × 1).
- `npm run build`: clean.
- Function-body byte-parity diff vs v0.3.47 (commit 4b56e0d): clean.
- lib.rs: 4035 → ~3942 lines (−93 net). app_init.rs: 138 lines new.

### Cross-review pré-Commit & Sync
Cross-review-v2 quadrilateral pendente (HARD GATE 2026-04-26).

## [v0.3.47] - 2026-05-02

Bundled hardening pass closing the remaining P1 set from Codex's audit (#2 DOCX/paste sanitization + CSP allowlist, #3 Cloudflare Secrets Store mode UI clarification) plus the trivial half of Gemini's clippy hygiene findings (5 of 7 fixes; the architectural ones — `too_many_arguments` × 7 and `items_after_test_module` — stay deferred for v0.4.0 with documented rationale).

### (1) DOCX / paste HTML sanitization (Codex #2 P1)
- [src/editor/posteditor/PostEditor.tsx](src/editor/posteditor/PostEditor.tsx) `handleWordUpload` — Mammoth-converted HTML now passes through `DOMPurify.sanitize(htmlResult.value, { ADD_ATTR: ['style', 'data-width'] })` before `editor.chain().focus().insertContent(...)`. Previously raw `htmlResult.value` was inserted into Tiptap; a malicious `.docx` (e.g. opened from email/web) could carry payloads such as `<img onerror=...>`, `javascript:` URLs, or unknown attributes that survive Tiptap's schema parsing. Markdown import (`markdownImport.ts:77`) already used DOMPurify; this mirrors that posture.
- [src/editor/posteditor/editor/extensions.ts](src/editor/posteditor/editor/extensions.ts) `WordPasteHandler.transformPastedHTML` — final return is now `DOMPurify.sanitize(clean, { ADD_ATTR: ['style', 'data-width'] })`. The existing Mso/xmlns/comment-strip layer is preserved (it removes Word bloatware), and DOMPurify is the defense-in-depth pass that drops event handlers, dangerous URL protocols, and embedded SVG with scripts. Inline styles for TextStyle/Color/FontFamily/TextIndent/EditorSpacing continue to be preserved via the explicit `ADD_ATTR: ['style']` whitelist.

### (2) Tauri webview CSP allowlist (Codex #2 P2)
- [src-tauri/tauri.conf.json](src-tauri/tauri.conf.json) `app.security.csp` — replaced `null` with a baseline allowlist:
  - `default-src 'self'` — block by default.
  - `img-src 'self' data: blob: https:` / `media-src 'self' data: blob: https:` — Tiptap inserts images via data URIs, blob URLs, and arbitrary HTTPS sources (operator can paste image URLs).
  - `font-src 'self' data:` — bundled fonts.
  - `style-src 'self' 'unsafe-inline'` — Tiptap's TextStyle/Color/FontFamily/FontSize/TextIndent/EditorSpacing all rely on inline `style` attributes.
  - `script-src 'self'` — **NO `unsafe-inline`**, the actual XSS hardening. Vite production bundles all JS into `dist/assets/*.js` served from `'self'`, so this should not break the app.
  - `frame-src 'self' https://www.youtube-nocookie.com https://www.youtube.com` — `CustomResizableYoutube` extension uses YouTube embeds with `nocookie: true`.
  - `connect-src 'self' ipc: http://ipc.localhost https://api.openai.com https://api.anthropic.com https://generativelanguage.googleapis.com https://api.deepseek.com https://api.cloudflare.com` — Tauri 2 IPC + the 4 provider APIs + Cloudflare D1/Secrets Store.

### (3) Cloudflare Secrets Store mode UI clarification (Codex #3 P1)
- [src/App.tsx:358](src/App.tsx#L358) `credentialStorageModes` Cloudflare row detail — was `'maestro_db + Secrets Store remoto'`, now `'maestro_db + Secrets Store remoto (execucao local exige MAESTRO_*_API_KEY em env)'`.
- [src/App.tsx:370-373](src/App.tsx#L370-L373) `storageModeSummaries.cloudflare` — expanded the detail copy to explain that the app does not fetch secrets from Cloudflare at runtime; for local execution the operator still needs `MAESTRO_OPENAI_API_KEY` / `MAESTRO_ANTHROPIC_API_KEY` / `MAESTRO_GEMINI_API_KEY` / `MAESTRO_DEEPSEEK_API_KEY` env vars (or local config). Cloudflare mode is a canonical-storage choice, not a path that unlocks local execution by itself. Codex's audit flagged the prior copy as "funcionalmente confuso"; this is the cheap option (a) from the operator decision matrix. Option (b) runtime fetch + (c) Worker/binding are still available for v0.4.x.

### (4) Trivial clippy hygiene (Gemini findings, 5 of 7)
- [src-tauri/src/app_paths.rs:48](src-tauri/src-tauri/src/app_paths.rs#L48) — dropped `return` on the last expression (`needless_return`).
- [src-tauri/src/human_logs.rs:115](src-tauri/src-tauri/src/human_logs.rs#L115) — `elapsed % 300 != 0` → `!elapsed.is_multiple_of(300)` (Rust 1.87+ stable, `manual_is_multiple_of`).
- [src-tauri/src/session_evidence.rs:400](src-tauri/src-tauri/src/session_evidence.rs#L400) — `((entry.size_bytes as usize + 2) / 3) * 4` → `(entry.size_bytes as usize).div_ceil(3) * 4` (Rust 1.73+ stable, `manual_div_ceil`). Math equivalence preserved (Base64 chars per 3 bytes, ceiling).
- [src-tauri/src/config_persistence.rs:65,79,93](src-tauri/src-tauri/src/config_persistence.rs#L65) — three `&PathBuf` → `&Path` parameter swaps for `persist_bootstrap_config`, `persist_ai_provider_config`, `persist_ai_provider_cloudflare_marker` (`ptr_arg`). Call sites continue to work via deref coercion. Import switched from `std::path::PathBuf` to `std::path::Path`.
- [src-tauri/src/lib.rs:3245](src-tauri/src-tauri/src/lib.rs#L3245) — `fs::create_dir_all(&session_dir.join(...))` → `fs::create_dir_all(session_dir.join(...))` (`needless_borrows_for_generic_args`).

### Tests + Gates
- `cargo test --locked --lib`: **88 passed** (same as v0.3.46, zero regressions). No new tests; existing coverage exercises all touched paths.
- `cargo clippy --locked --no-deps --all-targets`: **7 lib + 8 test warnings** (down from 11 + 14 in v0.3.46). Remaining 7 are the architecturally-deferred set: `too_many_arguments` × 7 (refactor target for v0.4.0 with `EditorialAgentContext` struct) + `items_after_test_module` × 1 in lib.rs:2542 (requires moving `pub fn run()` 87-line block above the `mod tests` declaration; will resolve naturally as lib.rs splits continue per `docs/code-split-plan.md`).
- `npm run build`: clean.

### Cross-review pré-Commit & Sync
Cross-review-v2 quadrilateral pendente (HARD GATE 2026-04-26).

### Out of scope (still deferred)
- `too_many_arguments` × 7 in `provider_runners.rs` + `provider_deepseek.rs`: requires Context Struct / Builder Pattern refactor; v0.4.0.
- `items_after_test_module` in `lib.rs:2542`: 87-line `pub fn run()` move; cosmetic; will resolve as splits continue.
- Codex #6 hardcoded model lists: real low-priority drift; defer until operator hits a stale fallback.
- biome.json: not in pipeline; configuration without integration is bloat.

## [v0.3.46] - 2026-05-02

Hardening pass closing the P0 set from Codex's read-only audit on HEAD `2ca92e7` (v0.3.45). All findings independently verified against current source before shipping. Three defensive fixes + CI gate that would have caught the v0.3.45 lockfile drift.

### (1) Rust reproducibility gate restored
- [src-tauri/Cargo.lock](src-tauri/Cargo.lock) — regenerated to match `Cargo.toml`. v0.3.45 shipped with the lockfile pinned to `0.3.44` because no Rust gate in CI surfaced the drift; `cargo check --locked` failed locally with `cannot update the lock file because --locked was passed to prevent this`.
- [.github/workflows/ci.yml](.github/workflows/ci.yml) — new `rust-gates` job on `windows-latest` runs `cargo check --locked --all-targets`, `cargo test --locked --lib`, `cargo clippy --locked --no-deps --all-targets`. CI now blocks pushes that desync the lockfile or break clippy, in addition to the existing `npm ci + npm run build` hygiene job.
- [.github/workflows/release.yml](.github/workflows/release.yml) — `npm run tauri -- build --ci --no-bundle -- --locked` so release builds inherit the same reproducibility guarantee.

### (2) Pipe buffer cap (defense in depth)
- [src-tauri/src/command_spawn.rs](src-tauri/src/command_spawn.rs) — new `pub(crate) const MAX_PIPE_BYTES: u64 = 64 * 1024 * 1024;` (64 MiB) cap on `read_pipe_to_end_counting_classified`. Past the cap, bytes continue to be drained from the OS pipe (so the child does not block on a full pipe and the timeout branch can still reap it cleanly), but they are not retained in the buffer. The atomic byte counter still reflects the full wire-bytes count.
- New `pipe_error` marker `stdout_truncated_oversize (cap=<bytes>; further output drained but not retained)` surfaces the cap value to the operator in the artifact. Truncation marker takes precedence over a late I/O error from the same pipe — the operator needs to see WHY the buffer was capped, not a downstream pipe close.
- Realistic motivation: operator-driven editorial sessions emit KB-MB of CLI output. A pathological hang (CLI stuck in error loop emitting megabytes/sec) would otherwise grow the buffer indefinitely until OOM.

### (3) Process tree teardown on Windows (timeout + stdin failure)
- [src-tauri/src/command_spawn.rs](src-tauri/src/command_spawn.rs) — new `pub(crate) fn kill_process_tree(child: &mut Child)`. On Windows, runs `taskkill /T /F /PID <child.id()>` to walk the descendant tree before falling back to the direct `child.kill()`. On non-Windows, keeps `child.kill()` (process-group SIGKILL when child was set up as a session leader).
- Two call sites swapped: stdin-write failure path (`run_resolved_command_observed`) and the timeout-elapsed path. Both previously called `child.kill()` directly, which on Windows leaks grandchildren when the peer is reached through `cmd.exe /C <peer>.cmd` (for `.cmd`/`.bat` resolves) or `powershell.exe -File` (for `.ps1` resolves). With this fix, the peer process is reaped along with its `cmd.exe`/`powershell.exe` wrapper.

### Tests — 5 new `#[cfg(test)] mod tests` invariants in `command_spawn`
- `pipe_reader_retains_short_payloads_without_truncation` — payloads ≤ MAX_PIPE_BYTES round-trip unchanged.
- `pipe_reader_caps_buffer_at_max_pipe_bytes_and_keeps_draining` — 64 MiB+4 KiB synthetic input → buffer capped at exactly MAX_PIPE_BYTES, marker contains the cap value, byte counter equals the full input size (proves drainage continued past the cap).
- `pipe_reader_classifies_io_error_when_no_truncation_yet` — first-error path still calls `classify_pipe_error`, no silent error swallow.
- `pipe_reader_truncation_marker_takes_precedence_over_late_io_error` — truncation cause wins over downstream pipe close, so the operator sees root cause, not noise.
- `max_pipe_bytes_is_64_mib` — pins the cap value to surface accidental edits in CI.

### Validation
- `cargo check --locked` clean.
- `cargo test --locked --lib`: **88 passed** (83 + 5 new). Zero regressions.
- `cargo clippy --locked --no-deps --all-targets`: 11 lib + 14 test warnings, same shape as v0.3.45 (no new warnings introduced; "items after a test module" is in `lib.rs:2542`, pre-existing).
- `npm run build`: clean.

### Cross-review pré-Commit & Sync
Cross-review-v2 quadrilateral pendente (HARD GATE 2026-04-26).

### Out of scope (audit findings deferred)
- **#2 DOCX/paste sanitization + CSP**: Mammoth output goes straight to Tiptap without DOMPurify, and CSP is `null`. Real high-severity finding but bundles two distinct fixes (sanitizer + CSP allowlist for Tauri webview); deferred to v0.3.47 after operator review of import flow regression risk.
- **#3 Cloudflare Secrets Store dead-end**: marker mode persists no usable credentials locally and runners can't read back from the Secrets Store API. Real, but the fix is either UI clarification (cheap) or runtime fetch (architectural — exposes the same token scope the mode was intended to avoid). Deferred to v0.3.47/v0.3.48 with operator decision on path.
- **#6 hardcoded model lists**: real but low-priority drift; deferred until operator hits a stale fallback.

## [v0.3.45] - 2026-05-02

Two operator-directed changes bundled in one release:

### (1) Default editorial prompt text rewritten — UX
- [src/App.tsx:821](src/App.tsx#L821) — replaced `'Escreva um artigo academico sobre o tema informado, seguindo integralmente o protocolo editorial ativo.'` with `'Escreva um artigo acadêmico sobre [...], seguindo rigorosa e integralmente o protocolo editorial ativo.'`. Adds proper Portuguese accent (`acadêmico`), inserts an explicit `[...]` slot for the operator to fill in the topic, and strengthens the protocol-adherence wording (`rigorosa e integralmente` instead of `integralmente`).

### (2) Upstream-bug stderr classification (operator log triage v0.3.45)
Triggered by operator catch on session log [maestro-2026-05-02T11-33-55Z-pid27440.ndjson](data/logs/maestro-2026-05-02T11-33-55Z-pid27440.ndjson) (running v0.3.43 binary): "vários erros silenciosos em background" — 8 errors + 4 warns where each peer hit a different upstream failure mode. Maestro was classifying them all as the generic `EMPTY_DRAFT` / `AGENT_FAILED_NO_OUTPUT` / `AGENT_FAILED_EMPTY`, which masked the actual root cause from the operator-facing artifact.

#### Changed (`src-tauri/src/editorial_agent_runners.rs`)
- New private helper `fn classify_upstream_cli_failure(name, stderr) -> Option<&'static str>` (~25 lines) that returns a more specific status code when the CLI's stderr matches a known upstream-bug fingerprint:
  - **`CODEX_WINDOWS_SANDBOX_UPSTREAM`** — Codex CLI 0.128.0+ on Windows runs the sandbox in PowerShell ConstrainedLanguage mode and trips on `Cannot set property` while resolving classifier state; process-tree teardown emits the Portuguese `ERRO: o processo "<pid>" nao foi encontrado` from `taskkill`. Documented in workspace memory `reference_codex_cli_sandbox_constrained_language.md`. Tracked upstream; deferred from cross-review-mcp v1.4.0 to v1.5.0+.
  - **`GEMINI_WORKSPACE_VIOLATION`** — Gemini CLI with `--skip-trust` resolves the workspace as the agent's CWD (`agent-runs/`) and refuses any file-system tool that touches the parent session directory; emits `Error executing tool list_directory: Path not in workspace` / `resolves outside the allowed workspace directories`.
- Classification site (run_editorial_agent body) now invokes `classify_upstream_cli_failure(name, &stderr)` BEFORE settling on the generic `EMPTY_DRAFT`/`AGENT_FAILED_NO_OUTPUT`/`AGENT_FAILED_EMPTY` codes. When the helper returns `Some("CODEX_WINDOWS_SANDBOX_UPSTREAM")` or `Some("GEMINI_WORKSPACE_VIOLATION")`, that status overrides the generic fallback. Tone stays `error`. The artifact note now explains the upstream root cause + what the operator can do (try another peer, or run outside the sandbox).
- Helper is intentionally scoped to `Codex` and `Gemini` agent names. Claude/DeepSeek failures continue to use the generic classifications because their failure modes are different (Claude CLI exit 1 with empty stderr; DeepSeek API silent empty response).

#### Tests — 5 new `#[test]` invariants in `editorial_agent_runners::tests`
- `classify_upstream_cli_failure_detects_codex_windows_sandbox_taskkill` — pins detection of the `ERRO: o processo` pattern.
- `classify_upstream_cli_failure_detects_codex_constrained_language` — pins detection of `ConstrainedLanguage` and `Cannot set property` patterns.
- `classify_upstream_cli_failure_detects_gemini_workspace_violation` — pins detection of `Path not in workspace` and `resolves outside the allowed workspace directories` patterns.
- `classify_upstream_cli_failure_returns_none_when_stderr_is_clean` — anti-regression: clean Codex/Gemini stderr (warning lines, version banners) does NOT trip the classifier.
- `classify_upstream_cli_failure_does_not_misclassify_other_agents` — anti-regression: even when Claude/DeepSeek/unknown stderr contains a substring matching the Codex/Gemini patterns, returns None to preserve the generic classification.

### What this does NOT fix (out of scope, documented as upstream)
- DeepSeek API silent empty response (3× in the operator log) — DeepSeek API returned 0 chars + exit 0 + 0 chars stderr in 32-44s. Likely DeepSeek-side rate limit or context-length silent truncation. Not detectable from the empty response itself.
- Claude CLI exit 1 with empty stderr (2× in the operator log) — true silent failure; no fingerprint to classify against. Stays as generic `AGENT_FAILED_NO_OUTPUT`.
- Codex Windows sandbox bug itself — upstream Codex CLI issue. Maestro now surfaces it clearly but cannot patch the third-party CLI.

### Validation
- `cargo test --lib`: **83 passed** (78 + 5 new).
- `cargo clippy --no-deps --all-targets`: 11 lib + 14 test warnings, all pre-existing (same shape as v0.3.44).
- `npm run build` (tsc --noEmit + vite build): clean, 2434 modules.

## [v0.3.44] - 2026-05-02

Pure refactor — no behavior change. Continues `docs/code-split-plan.md` migration step 3 (provider API surfaces) by extracting the per-provider API request payload builders + native attachment support detection.

### Changed (extracted to `src-tauri/src/api_payloads.rs`, ~214 lines incl. doc header, 11 items: 1 const + 4 `pub(crate)` fns + 6 module-private fns)
- `pub(crate) const API_NATIVE_ATTACHMENT_MAX_FILE_BYTES: u64 = 20 * 1024 * 1024` — single-file inline-base64 payload cap (20 MiB).
- `pub(crate) fn api_input_estimate_chars(prompt, attachments, provider) -> usize` — input-cost preflight estimator. Sums prompt chars + per-attachment overhead (base64 chars + filename chars + media-type chars + 96 bytes JSON envelope).
- `fn provider_supports_native_attachment(provider, entry) -> bool` (module-private) — dispatcher; openai/anthropic/gemini → corresponding helper; unknown → false.
- `fn openai_api_attachment_supported(entry) -> bool` (module-private) — image OR known document, gated by payload cap.
- `fn openai_api_file_attachment_supported(entry) -> bool` (module-private) — known document attachment proxy.
- `fn anthropic_api_attachment_supported(entry) -> bool` (module-private) — image OR PDF, gated by payload cap.
- `fn gemini_api_attachment_supported(entry) -> bool` (module-private) — image OR audio OR video OR PDF OR text-like OR known document, gated by payload cap.
- `fn attachment_within_native_payload_cap(entry) -> bool` (module-private) — single point of truth for the 20 MiB limit.
- `pub(crate) fn openai_api_input(prompt, attachments) -> Result<Value, String>` — Responses API input shape (input_text + input_image + input_file).
- `pub(crate) fn anthropic_api_user_content(prompt, attachments) -> Result<Value, String>` — Messages API user content shape (text + image base64 + document base64+title).
- `pub(crate) fn gemini_api_user_parts(prompt, attachments) -> Result<Vec<Value>, String>` — generateContent parts shape (text + inline_data with mime_type+base64).

### Re-export shim in `lib.rs` (matches v0.3.40+ pattern)
```rust
pub(crate) use crate::api_payloads::{
    anthropic_api_user_content, api_input_estimate_chars, gemini_api_user_parts, openai_api_input,
};
#[cfg(test)]
use crate::api_payloads::API_NATIVE_ATTACHMENT_MAX_FILE_BYTES;
```

Preserves call sites in `provider_runners.rs` and `provider_deepseek.rs` without per-file edits. The 4 module-private support helpers and `provider_supports_native_attachment` + `attachment_within_native_payload_cap` are not re-exported (minimal surface). The constant is `#[cfg(test)]`-gated because only `lib.rs::tests` exercises the cap directly; production code goes through `api_payloads.rs` internally.

### Cleanup in `lib.rs`
- Trimmed `session_evidence` import block from 13 items to 3 (`process_session_evidence`, `AttachmentManifestEntry`, `PromptAttachmentRequest`). The 10 removed (`attachment_base64`, `attachment_data_url`, `attachment_payload_base64_chars`, `is_audio_attachment`, `is_image_attachment`, `is_known_document_attachment`, `is_pdf_attachment`, `is_text_like_attachment`, `is_video_attachment`, `normalized_attachment_media_type`) now live inside `api_payloads.rs`.
- Removed orphaned doc comment (lines 2379-2388 in pre-fix lib.rs) that referenced `filter_existing_agents_to_active_set` — that function lives in `editorial_helpers.rs` since v0.3.24; the doc was orphaned debris from that extraction. Removing it dropped one clippy `empty_line_after_doc_comments` warning.

### Validation
- `cargo test --lib`: **78 passed** (zero regression vs v0.3.43).
- `cargo clippy --no-deps --all-targets`: **11 lib + 14 test warnings** (one fewer than v0.3.43; the orphaned-doc fix saved a warning).
- `npm run build` (tsc --noEmit + vite build): clean.
- `lib.rs`: 4177 → 4049 lines (−128 net).
- `api_payloads.rs`: 214 lines new.
- Function-body byte-parity diff vs v0.3.43 (commit f7beeb7): clean.

## [v0.3.43] - 2026-05-02

Pure refactor — no behavior change. Continues `docs/code-split-plan.md` migration step 3 (provider routing tail) + step 4 (env helpers) by extracting the per-agent provider routing and env-var lookup helpers.

### Changed (extracted to `src-tauri/src/provider_routing.rs`, ~185 lines incl. doc header, 8 functions)
- `pub(crate) fn api_cli_for_agent(agent_key) -> &'static str` — agent_key → static label like "anthropic-api". Used as the `cli` field in NDJSON `session.agent.started`/`session.agent.finished` log records when a peer runs through an API runner.
- `pub(crate) fn provider_label_for_agent(agent_key) -> &'static str` — agent_key → human label "Anthropic / Claude" etc. Used in PT-BR error strings and UI surfaces.
- `pub(crate) fn provider_remote_present(config, agent_key) -> bool` — reads `*_api_key_remote` flags on `AiProviderConfig` to indicate Cloudflare-managed credential presence.
- `pub(crate) fn provider_key_for_agent(config, agent_key) -> Option<(String, String)>` — config_value first, then env-var fallback via `effective_provider_key`. Returns `(value, source_label)`.
- `pub(crate) fn first_env_value(candidates) -> Option<(String, String, String)>` — walks env-var candidate list returning first present as `(name, scope, value)`.
- `pub(crate) fn env_value_with_scope(name) -> Option<(String, String)>` — `std::env::var` first ("process"), then on Windows `HKCU\Environment` ("user") and `HKLM\SYSTEM\CurrentControlSet\Control\Session Manager\Environment` ("machine") via `reg.exe query`.
- `fn windows_registry_env_value(key, name)` (Windows-only, module-private) — `reg.exe` parser that extracts the value column for a given REG_* type row. Calls `crate::hidden_command("reg.exe")` for non-window-popping spawn.
- `pub(crate) fn effective_provider_key(config_value, env_candidates) -> Option<(String, String)>` — config_value (trimmed) wins; otherwise `first_env_value` over the candidates with source label `<env_name>:<scope>`.

### Re-export shim in `lib.rs` (matches v0.3.40/v0.3.41 pattern)
```rust
pub(crate) use crate::provider_routing::{
    api_cli_for_agent, effective_provider_key, env_value_with_scope, first_env_value,
    provider_key_for_agent, provider_label_for_agent, provider_remote_present,
};
```

Preserves all unqualified call sites in 6 sibling modules (`ai_probes.rs`, `cloudflare.rs`, `editorial_agent_runners.rs`, `provider_config.rs`, `provider_deepseek.rs`, `provider_runners.rs`) without per-file edits. The Windows-only `windows_registry_env_value` is consumed only inside `provider_routing.rs` and is not re-exported.

### Stayed in `lib.rs`
- `hidden_command` — the Windows process-spawn primitive that `windows_registry_env_value` calls (also used by Tauri command handlers and other spawn paths).
- `AiProviderConfig` struct definition.

### Validation
- `cargo test --lib`: **78 passed** (zero regression vs v0.3.42).
- `cargo clippy --no-deps --all-targets`: 12 lib + 15 test warnings, all pre-existing (same shape as v0.3.42).
- `npm run build` (tsc --noEmit + vite build): clean.
- `lib.rs`: 4297 → 4177 lines (−120 net).
- `provider_routing.rs`: 185 lines new (doc header + 8 functions).
- Function-body byte-parity diff vs v0.3.42 (commit 0f672cf): clean.

## [v0.3.42] - 2026-05-02

Behavior fix — operator catch on session log [maestro-2026-05-02T10-54-07Z-pid1688.ndjson](data/logs) (running v0.3.39 binary): the v0.3.32 B20 fix was incomplete on the backend. With operator's form blank on resume (intent: "no cap, sessão livre"), the saved contract's prior caps were still being silently re-applied. All releases v0.3.32 through v0.3.41 carry this same regression.

### Root cause
[src-tauri/src/lib.rs:1575-1586](src-tauri/src/lib.rs#L1575-L1586) — the inline resolver called `request.max_session_cost_usd.or_else(|| saved_contract...max_session_cost_usd)`. v0.3.32 fixed the frontend (`startResumeSession` no longer pre-populates the form), but this `or_else` backend fallback meant: frontend sends None → backend falls through to the saved 5.0 cap → DeepSeek spawns with `cost_limit_usd: 5.0` even though operator wanted unlimited.

### Reproduction in operator's log
- L19 `session.resume.contract_applied`: `requested_max_session_cost_usd: None, requested_max_session_minutes: None` (form blank).
- L21 `session.editorial.requested`: `max_session_cost_usd: None, max_session_minutes: None` (IPC request to backend carried None).
- L23 `session.editorial.active_agents_resolved`: `max_session_cost_usd_requested: 5.0, max_session_cost_usd_saved: 5.0, max_session_cost_usd_source: 'request'`. The backend already merged `request.or_else(saved)` BEFORE building this log payload, so `_requested` reports 5.0 instead of None.
- L26 `session.agent.started` for DeepSeek: `cost_limit_usd: 5.0`. Saved cap silently applied.

### Fix
Replaced the `or_else` fallback with direct request pass-through:

```rust
// Before (v0.3.32–v0.3.41):
let max_session_cost_usd = sanitize_optional_positive_f64(
    request.max_session_cost_usd.or_else(|| {
        saved_contract.as_ref().and_then(|contract| contract.max_session_cost_usd)
    }),
);
let max_session_minutes = sanitize_optional_positive_u64(
    request.max_session_minutes.or_else(|| {
        saved_contract.as_ref().and_then(|contract| contract.max_session_minutes)
    }),
);

// After (v0.3.42):
let max_session_cost_usd = sanitize_optional_positive_f64(request.max_session_cost_usd);
let max_session_minutes = sanitize_optional_positive_u64(request.max_session_minutes);
```

Per the 2026-05-02 operator directive ("cada nova sessão, mesmo que seja sessão retomada, deve ser livre para que o usuário defina novos valores ou não"), the request alone is the source of truth. None means "no cap" — the saved contract's value is for historical reference only and MUST NOT be silently re-applied.

### Behavior matrix (post-fix)
| Scenario | Operator form | Effective cap |
|---|---|---|
| Fresh start, blank form | None | None (unlimited) |
| Fresh start, "10" | Some(10.0) | 10.0 |
| Resume (saved=5), blank form | None | **None (unlimited) ← was 5.0 before** |
| Resume (saved=5), blank form, retain cap | "5" | 5.0 |
| Resume (saved=5), bumped form | "10" | 10.0 |
| Resume (saved=5), explicitly cleared | None | None (unlimited) |

### Provider mode toggle re-verified (NOT a regression)
The same operator log shows `provider_mode='hybrid'` selected and the v0.3.38 routing fix working correctly: L26 DeepSeek → `cli: 'deepseek-api'` (API), L30/41/44 Claude/Codex/Gemini → CLI peers. The "all agents via API" complaint surfaced alongside the cost-cap bug; the cost cap was the real issue (DeepSeek's `cost_limit_usd: 5.0` was the surface symptom), not the routing.

### Validation
- `cargo test --lib`: **78 passed** (zero regression vs v0.3.41).
- `npm run build`: clean.

### Operational notes
- Pure behavior fix — no module split. Continues the v0.3.42 split-coding plan only after this hotfix ships. Cli/provider routing extraction (`provider_routing.rs`) deferred.
- Affected versions: **all releases v0.3.32 through v0.3.41 carry this bug**. Operators on those binaries should upgrade to v0.3.42.

## [v0.3.41] - 2026-05-02

Pure refactor — no behavior change. Continues `docs/code-split-plan.md` migration step 2 by extracting the bootstrap and AI provider config persistence layer (disk + Cloudflare).

### Changed (extracted to `src-tauri/src/config_persistence.rs`, ~311 lines incl. doc header, 7 functions)
- `pub(crate) fn persist_bootstrap_config(path, config) -> Result<(), String>` — atomic JSON write to disk via the `checked_data_child_path` safety gate from `app_paths.rs`.
- `pub(crate) fn persist_ai_provider_config(path, config) -> Result<(), String>` — same shape, for the AI provider config.
- `pub(crate) fn persist_ai_provider_cloudflare_marker(path, config)` — writes a marker JSON to disk that records `credential_storage_mode = "cloudflare"` and which remote secrets are present, with the actual API keys cleared. Used when the operator opts in to Cloudflare-managed secrets so the local file no longer holds plaintext.
- `pub(crate) fn persist_ai_provider_config_to_cloudflare(config, request)` — full upload path: ensures D1 database + Secrets Store via `cloudflare.rs` ensure_* helpers, upserts per-provider secrets, writes the metadata row to D1.
- `pub(crate) fn enrich_ai_provider_config_from_cloudflare(config, bootstrap)` — best-effort merge of remote `provider_mode` / remote-secret-present flags / store id+name into a locally-loaded config (no error propagation; falls through when the remote read fails).
- `pub(crate) fn read_ai_provider_cloudflare_metadata(bootstrap)` — reads the JSON value blob from `maestro_settings WHERE key='ai.providers'` in D1 and rebuilds an `AiProviderConfig` with `credential_storage_mode="cloudflare"`, remote presence flags, store id+name, and per-provider tariff rates.
- `fn json_find_first_string(value, key)` (module-private) — recursive helper to find the first string value for a given key anywhere in a serde_json `Value`. No callers outside this module.

### Visibility upgrade in `lib.rs` (cross-module access required)
- `pub(crate) struct BootstrapConfig` + 9 fields (`schema_version`, `credential_storage_mode`, `cloudflare_account_id`, `cloudflare_api_token_source`, `cloudflare_api_token_env_var`, `cloudflare_persistence_database`, `cloudflare_secret_store`, `windows_env_prefix`, `updated_at`).

### Re-export shim in `lib.rs` (matches v0.3.40 `provider_config.rs` pattern)
- `pub(crate) use crate::config_persistence::{enrich_ai_provider_config_from_cloudflare, persist_ai_provider_cloudflare_marker, persist_ai_provider_config, persist_ai_provider_config_to_cloudflare, persist_bootstrap_config};` — preserves all unqualified call sites without per-file edits.
- Items consumed only inside `config_persistence.rs` (`json_find_first_string`, `read_ai_provider_cloudflare_metadata`) are NOT re-exported (minimal surface; `read_ai_provider_cloudflare_metadata` has no caller in lib.rs after extraction since `enrich_ai_provider_config_from_cloudflare` calls it internally).

### Cleanup in `lib.rs`
- Trimmed the cloudflare import block from 10 items to 2 (`run_cloudflare_probe`, `token_source_label` — the only ones still consumed in lib.rs production code). The 8 removed (`ai_provider_secret_values`, `cloudflare_client`, `cloudflare_get`, `cloudflare_post_json`, `cloudflare_result_id_for_name`, `cloudflare_token_from_provider_request`, `ensure_cloudflare_d1_database`, `ensure_cloudflare_secret_store`, `upsert_ai_provider_secrets`, `write_ai_provider_metadata_to_cloudflare`) now live inside `config_persistence.rs`. `ai_provider_secret_values` is `#[cfg(test)]`-gated in lib.rs (one test still uses it directly).

### Stayed in `lib.rs`
- `read_bootstrap_config` and `read_ai_provider_config` Tauri commands (registry boundary; consume the helpers here via crate-level imports).
- `BootstrapConfig` and `AiProviderConfig` struct definitions themselves.
- `first_env_value` and `env_value_with_scope` (used outside the persistence layer).

### Validation
- `cargo test --lib`: **78 passed** (zero regression vs v0.3.40).
- `cargo clippy --no-deps --all-targets`: 12 lib + 15 test warnings, all pre-existing (same shape as v0.3.40, no new warnings).
- `npm run build` (tsc --noEmit + vite build): clean.
- `lib.rs`: 4527 → 4295 lines (−232 net; 7 function bodies removed plus cloudflare import shrink).
- `config_persistence.rs`: 311 lines new (doc header + 7 functions).
- Function-body byte-parity diff vs v0.3.40 (commit 254e5a3): clean.

## [v0.3.40] - 2026-05-02

Pure refactor — no behavior change. Continues `docs/code-split-plan.md` migration step 5 by extracting the AI provider config validation, sanitization, mode-normalization, and routing layer.

### Changed (extracted to `src-tauri/src/provider_config.rs`, ~340 lines incl. doc header + 5 tests, 11 functions)
- `pub(crate) fn normalize_storage_mode` / `normalize_provider_mode` / `normalize_cloudflare_token_source` — collapse free-form strings into closed-enum string literals (unknown values fall to `"local_json"` / `"hybrid"` / `"prompt_each_launch"`).
- `pub(crate) fn sanitize_optional_secret` (trims, caps to 4096 chars, drops empties) and `pub(crate) fn sanitize_optional_cost_rate` (filters `Option<f64>` to finite > 0 and ≤ 10_000.0).
- `pub(crate) fn sanitize_ai_provider_config` — full per-field config builder used by the read/write Tauri commands and by the env-merge layer.
- `pub(crate) fn merge_ai_provider_env_values` + `pub(crate) fn provider_env_value` — env var fallback when individual API keys are absent in the config (reads `MAESTRO_<PROVIDER>_API_KEY` then `<PROVIDER>_API_KEY`).
- `pub(crate) fn provider_cost_rates_from_config` — builds the per-agent `ProviderCostRates` for cost-guard preflight; returns a Portuguese error string keyed to the operator UI when a tariff is missing.
- `pub(crate) fn api_provider_for_agent` — agent_key → provider_label mapping (`"claude" → "anthropic"`, `"codex" → "openai"`, etc.).
- `pub(crate) fn should_run_agent_via_api` — the v0.3.38 routing decision: false when no API provider OR `provider_mode == "cli"`; true when `"api"`; identity-deterministic hybrid (DeepSeek-only) otherwise.

### Tests migrated (5 unit tests moved verbatim to `provider_config::tests`)
- `ai_provider_config_trims_empty_secret_fields` — pins `sanitize_ai_provider_config` schema/sanitization output.
- 4 mode-routing invariants from v0.3.38: `should_run_agent_via_api_api_mode_routes_all_to_api`, `should_run_agent_via_api_cli_mode_routes_all_to_cli`, `should_run_agent_via_api_hybrid_mode_routes_only_deepseek_to_api`, `should_run_agent_via_api_hybrid_mode_is_deterministic_regardless_of_keys`.

### Re-export shim in `lib.rs` (matches the v0.3.34 `sanitize.rs` pattern)
- `pub(crate) use crate::provider_config::{api_provider_for_agent, merge_ai_provider_env_values, normalize_cloudflare_token_source, normalize_storage_mode, provider_cost_rates_from_config, sanitize_ai_provider_config, should_run_agent_via_api};` — keeps existing `crate::sanitize_ai_provider_config` and similar unqualified call sites working without per-file edits.
- Items consumed exclusively inside `provider_config.rs` (`normalize_provider_mode`, `provider_env_value`, `sanitize_optional_cost_rate`, `sanitize_optional_secret`) are NOT re-exported — minimal surface.

### Cleanup in `lib.rs`
- `ProviderCostRates` import gated `#[cfg(test)]` — only the cost-rates test still uses it directly in `lib.rs::tests`; production code calls `provider_cost_rates_from_config` from `provider_config.rs` which holds its own `use session_controls::ProviderCostRates`.

### Stayed in `lib.rs`
- `AiProviderConfig`, `BootstrapConfig` structs — still consumed by the Tauri command boundary.
- The CLI peer routing helpers (`api_cli_for_agent`, `provider_label_for_agent`, `provider_remote_present`, `provider_key_for_agent`) — already `pub(crate)` and tightly coupled with `provider_runners.rs`.
- `effective_provider_key` — used by both `provider_key_for_agent` (lib.rs) and `provider_runners.rs`; not part of the routing layer.

### Validation
- `cargo test --lib`: **78 passed** (zero regression vs v0.3.39).
- `cargo clippy --no-deps --all-targets`: 12 lib + 15 test warnings, all pre-existing (same shape as v0.3.39, no new warnings).
- `npm run build` (tsc --noEmit + vite build): clean.
- `lib.rs`: 4788 → 4527 lines (−261 net; 11 function bodies + 5 tests removed).
- `provider_config.rs`: 340 lines new (doc header + 11 `pub(crate)` functions + 5 tests).
- Function-body byte-parity diff vs v0.3.39 (commit dc60061): clean (only differences are the use block at the top of the new module, the `pub(crate) fn` decoration, and the wrapping `mod tests` for the migrated tests).

## [v0.3.39] - 2026-05-02

Pure refactor — no behavior change. Continues `docs/code-split-plan.md` migration step 5 by extracting the CLI adapter smoke probe machinery into a dedicated module.

### Changed (extracted to `src-tauri/src/cli_adapter.rs`, ~153 lines with doc header, 2 functions)
- `pub(crate) fn cli_adapter_specs(request: &CliAdapterSmokeRequest) -> Vec<CliAdapterSpec>` — builds the 3-element spec table (Claude with `--print --output-format text --permission-mode dontAsk`, Codex with `exec --skip-git-repo-check --sandbox read-only --color never`, Gemini with `--prompt --output-format text --approval-mode yolo --skip-trust`); 90s per-CLI timeout.
- `pub(crate) fn run_cli_adapter_probe(spec: CliAdapterSpec) -> CliAdapterProbeResult` — single-spec runner: resolves the command against effective PATH (returns `blocked` tone with status "CLI nao encontrada no PATH efetivo" when missing), invokes `run_resolved_command_with_timeout`, classifies outcome (timeout/ok+marker/ok-without-marker/nonzero-exit).

### Visibility upgrades in `lib.rs` (cross-module access required)
- `pub(crate) struct CliAdapterSmokeRequest` + 5 fields (run_id, prompt_chars, protocol_name, protocol_lines, protocol_hash).
- `pub(crate) struct CliAdapterSmokeResult` + 3 fields (run_id, agents, all_ready).
- `pub(crate) struct CliAdapterProbeResult` + 7 fields (name, cli, tone, status, duration_ms, exit_code, marker_found).
- `pub(crate) struct CliAdapterSpec` + 5 fields (name, command, marker, args, timeout).

### Stayed in `lib.rs`
- `run_cli_adapter_smoke` Tauri command wrapper — lives on the `#[tauri::command]` registry boundary and orchestrates the 3-CLI parallel spawn loop via `thread::spawn(move || run_cli_adapter_probe(spec))` joining results back into `CliAdapterSmokeResult`.

### Cleanup in `lib.rs`
- Removed unused `Instant` import (only `cli_adapter.rs` uses it now).
- Collapsed `command_spawn` import: removed `run_resolved_command_with_timeout` (only `cli_adapter.rs` calls it now), single-line use statement.

### Validation
- `cargo test --lib`: **78 passed** (zero regression vs v0.3.38).
- `cargo clippy --no-deps --all-targets`: 12 lib + 15 test warnings, all pre-existing (same shape as v0.3.38).
- `npm run build` (tsc --noEmit + vite build): clean.
- `lib.rs`: 4902 → 4788 lines (−114 net; 111 sed-deleted, 3 blank-line collapse, 1 mod line + 1 use line + minor cleanup adjustments).
- Function-body byte-parity diff vs v0.3.38 (commit b7509b9): clean (only differences are the use block at the top of the new module and the wrapping `pub(crate) fn` decoration).

## [v0.3.38] - 2026-05-02

Behavior fix — operator catch on session log [maestro-2026-05-02T09-01-33Z-pid7592.ndjson](data/logs): provider mode toggle (Hibrido/CLI/API) was not being respected for DeepSeek. Operator selected `provider_mode='cli'`, but DeepSeek still ran via `https://api.deepseek.com/chat/completions` while Claude/Codex/Gemini correctly resolved to local CLI binaries.

### Root cause
Both backend and frontend short-circuited DeepSeek to API regardless of mode:
- [src-tauri/src/lib.rs:1664](src-tauri/src/lib.rs#L1664) `should_run_agent_via_api` returned `true` when `agent_key == "deepseek"` before checking `provider_mode`.
- [src/App.tsx:926](src/App.tsx#L926) `agentUsesApi` had the same shortcut.

The shortcut was a workaround for the absence of a DeepSeek CLI integration in maestro-app (deferred from cross-review-mcp v1.4.0 → v1.5.0; the `deepseek-cli` direct integration was rejected over command-injection via cmd.exe newline truncation). The shortcut violated the explicit operator selection and was never documented in the settings UI.

### Fixed — provider mode is now strict and deterministic
- **API**: all 4 peers via API.
- **Hybrid**: DeepSeek via API + Claude/Codex/Gemini via CLI, **always**, regardless of which API keys are configured. Hybrid is now defined by agent identity, not credential availability — its sole purpose is to let DeepSeek (no CLI) join an otherwise CLI-driven session.
- **CLI**: all CLI; DeepSeek button disabled in the agent toggles with tooltip "DeepSeek so roda via API. Troque para Hibrido ou API para incluir."

### Backend — [src-tauri/src/lib.rs](src-tauri/src/lib.rs)
- `should_run_agent_via_api`: removed DeepSeek shortcut. Hybrid branch now `agent_key == "deepseek"` (deterministic) instead of `provider_key_for_agent(...).is_some()`.

### Frontend — [src/App.tsx](src/App.tsx)
- `agentUsesApi`: mirrors the new logic.
- `chooseProviderMode`: when switching to CLI, auto-removes DeepSeek from `activeAgents` and reassigns `initialAgent='claude'` if it was DeepSeek.
- New `useEffect([providerMode, activeAgents, initialAgent])` defense-in-depth: catches config-load AND saved-contract restore paths that call `setActiveAgents`/`setInitialAgent` directly while `providerMode` is already `'cli'`. Reads state directly (not via setState updater closure) so `react-hooks/preserve-manual-memoization` accepts the deps; both setState calls are guarded so no render loop is possible. Dep-array tightening was raised in cross-review-v2 R1 by codex + deepseek (both NEEDS_EVIDENCE) and shipped in R2.
- Initial-agent picker (line ~2814) and peer toggle row (line ~2837): both gate the DeepSeek button via `cliBlocksDeepseek` flag.
- Settings UI text (line ~3528): rewritten to explicitly explain the new semantics — "API roda os 4 peers via provedores oficiais. Hibrido reserva DeepSeek para API (nao tem CLI) e Claude, Codex, Gemini para CLI, sempre, independentemente das chaves. CLI roda os 3 peers com CLI; DeepSeek fica desabilitado porque nao possui integracao CLI."

### Tests — 4 new `#[test]` invariants in [src-tauri/src/lib.rs](src-tauri/src/lib.rs)
- `should_run_agent_via_api_api_mode_routes_all_to_api`
- `should_run_agent_via_api_cli_mode_routes_all_to_cli`
- `should_run_agent_via_api_hybrid_mode_routes_only_deepseek_to_api`
- `should_run_agent_via_api_hybrid_mode_is_deterministic_regardless_of_keys` — proves keyed-but-CLI-routed for non-DeepSeek peers (anti-regression: the pre-v0.3.38 behavior would have routed any keyed agent to API in hybrid).

### Validation
- `cargo test`: **78 passed** (74 + 4 new).
- `cargo clippy --no-deps --all-targets`: no new warnings (12 lib + 13 test warnings, all pre-existing).
- `npm run build` (tsc --noEmit + vite build): clean, 1.06s, 2434 modules transformed.

### Operational notes
- Pure behavior fix — no module split. Cli adapter split (`cli_adapter.rs`) deferred to v0.3.39.
- Defense in depth: even if a stale config or contract bypasses the frontend gates, the backend `should_run_agent_via_api` returns `false` for DeepSeek in CLI mode, the CLI runner emits a clear `CLI nao encontrada no PATH efetivo` for `deepseek` (no resolved binary), and the operator sees an explicit failure instead of a silent API call.
- Cross-review-v2 quadrilateral session `d24674a0-898f-4258-8714-f7a49cf6f1f3` converged `unanimous_ready_after_R2_fix_for_useeffect_deps` after 2 rounds: R1 had gemini READY but codex + deepseek NEEDS_EVIDENCE on the useEffect dep-array gap (saved-contract restore could re-inject DeepSeek without flipping `providerMode`); R2 shipped the dep-array tightening + concrete code snippets + raw test output and converged with all 3 peers READY. Cross-review-v1 was tried first per operator request but aborted: codex hit the upstream Windows sandbox bug ("ERRO: o processo... não foi encontrado") and gemini probe failed; v1 session `fb6eee68-a4eb-4f91-ae38-a9fe1044b926` finalized aborted with reason `operator_switch_to_v2_for_diagnostic_separation` for later v1 transport diagnostic.

## [v0.3.37] - 2026-05-02

Pure refactor — no behavior change. Continues migration step 5 by extracting the `ata-da-sessao.md` (session minutes) text generators into a dedicated module.

### Changed (extracted to `src-tauri/src/session_minutes.rs`, ~148 lines with doc header, 2 functions)
- `pub(crate) fn build_session_minutes` — markdown body of `ata-da-sessao.md` (header, per-agent bullets, "Decisao" section).
- `pub(crate) fn build_blocked_minutes_decision` — non-unanimous decision section: counts READY reviews / operational failures / editorial divergences, lists most recent 8 of each.

### Other
- Removed unused `all_agent_keys` import from lib.rs (only `build_session_minutes` consumed it).

### Validation
- `cargo test`: 74 passed.
- `npm run typecheck` + `npm run build`: clean.
- `lib.rs`: 4956 → 4836 lines (−120 net; 115 sed-deleted, 5 mod/use lines added).
- ZERO-line byte-parity diff vs v0.3.36 (commit e199c1b).

## [v0.3.36] - 2026-05-02

Pure refactor — no behavior change. Continues migration step 5 by extracting the per-spec agent dispatchers + the CLI-path editorial agent runner.

### Changed (extracted to `src-tauri/src/editorial_agent_runners.rs`, ~433 lines with doc header, 3 functions)
- `pub(crate) fn run_editorial_agent_for_spec` — top-level dispatcher (API peer vs CLI runner).
- `fn run_provider_api_agent` (private) — match-by-spec.key dispatcher to the 4 API peer runners.
- `fn run_editorial_agent` (private) — CLI-path runner with sidecar input prep (>48 KB), CLI_NOT_FOUND short-circuit, RUNNING placeholder, spawn-via-`run_resolved_command_observed`, status classification (READY / NOT_READY / DRAFT_CREATED / EMPTY_DRAFT / AGENT_FAILED_EMPTY / AGENT_FAILED_NO_OUTPUT / EXEC_ERROR), artifact emission.

### `pub(crate)` visibility upgrades in `lib.rs`
- `pub(crate) fn api_cli_for_agent` (consumed by `run_provider_api_agent` for the `API_PROVIDER_NOT_SUPPORTED` fallback).

### Defensive additions
- Two `#[allow(clippy::too_many_arguments)]` annotations on `run_editorial_agent_for_spec` (11 args) and `run_provider_api_agent` (10 args) following the v0.3.22 sibling convention.

### Validation
- `cargo test`: 74 passed.
- `npm run typecheck` + `npm run build`: clean.
- `lib.rs`: 5302 → 4956 lines (−346 net; 353 sed-deleted via `'3123,3364d;2869,2979d'`).
- 2-range byte-parity diff vs v0.3.35 (commit 05a7a0f): both ranges exit=0 after stripping defensive `#[allow]` and pub(crate) prefix.

## [v0.3.35] - 2026-05-02

Pure refactor — no behavior change. Continues migration step 5 by extracting the child-process spawn machinery (timeout, progress logging, pipe readers, command builders, environment policy) into a dedicated module.

### Changed (extracted to `src-tauri/src/command_spawn.rs`, ~349 lines with doc header, 8 items)
- `pub(crate) fn command_check` — diagnostic helper called by `dependency_preflight`.
- `pub(crate) struct CommandProgressContext<'a>` + 6 fields.
- `pub(crate) fn run_resolved_command_with_timeout`, `run_resolved_command_observed` — spawn loop with 250ms poll, 30s progress emit, optional timeout.
- `fn read_pipe_to_end_counting_classified` (private) — pipe reader with shared atomic byte counter.
- `pub(crate) fn classify_pipe_error` — Windows-aware classifier (raw_os_error 109/232/233 + std `ErrorKind` variants).
- `fn resolved_command_builder` (private) — Windows: `.cmd`/`.bat` → `cmd.exe /C`; `.ps1` → `powershell.exe -NoProfile -ExecutionPolicy Bypass -File`; else direct.
- `pub(crate) fn apply_editorial_agent_environment` — UTF-8 env (`PYTHONIOENCODING`/`PYTHONUTF8`/`LC_ALL`/`LANG`) + `GEMINI_CLI_TRUST_WORKSPACE` for gemini stem (B1 from v0.3.15).

### `pub(crate)` visibility upgrades in `lib.rs`
- `pub(crate) struct TimedCommandOutput` + 5 fields (output / duration_ms / timed_out / stdout_pipe_error / stderr_pipe_error).
- `pub(crate) fn hidden_command` — only entry that funnels through `apply_hidden_window_policy` per the v0.3.16 `clippy.toml` `disallowed-methods` policy.
- `pub(crate) fn command_working_dir_for_output` — wrapper around the output_path's parent dir.
- `pub(crate) fn log_editorial_agent_spawned`, `log_editorial_agent_running` — NDJSON helpers tightly coupled with the editorial orchestration log schema.

### Validation
- `cargo test`: 74 passed.
- `npm run typecheck` + `npm run build`: clean.
- `lib.rs`: 5581 → 5302 lines (−279 net; 282 sed-deleted, 5 mod/use/import lines added).
- ZERO-line byte-parity diff vs v0.3.34 (commit e00538e).

## [v0.3.34] - 2026-05-02

Pure refactor — no behavior change. Continues migration step 5 by extracting the foundational text-sanitization + secret-redaction helpers into a dedicated module.

### Changed (extracted to `src-tauri/src/sanitize.rs`, ~170 lines with doc header, 7 items)
- `pub(crate) fn sanitize_short` — strips to ASCII alphanumerics + `_-.:`.
- `pub(crate) fn sanitize_text` — redact + char-count truncate.
- `pub(crate) fn truncate_text_head_tail` — head + tail preservation for large stderr/stdout.
- `pub(crate) fn sanitize_value` — recursive JSON sanitizer with depth + array (80) + object (120) caps.
- `pub(crate) fn should_redact_key` (test-exposed) — keyname-based redaction predicate with safe-suffix allowlist.
- `pub(crate) fn redact_secrets` — replaces matches of the secret regex with `<redacted>`.
- `secret_value_regex` (private) — `OnceLock<Regex>` cache covering sk-ant/sk_live/sk-/cfut_/cfat_/cfk_/xox[baprs]/gh[pousr]/AIza/re_/AKIA/PEM patterns.

### Re-export shim in `lib.rs`
- `pub(crate) use crate::sanitize::{ sanitize_short, sanitize_text, sanitize_value, truncate_text_head_tail }` plus a `#[cfg(test)] pub(crate) use { redact_secrets, should_redact_key }`. This preserves the existing `crate::sanitize_text` import path across all 19 sibling modules — zero downstream `use` changes required.

### Other
- Removed unused `regex::Regex` and `std::sync::OnceLock` imports from lib.rs (the only callers moved with the regex helpers).

### Validation
- `cargo test`: 74 passed.
- `npm run typecheck` + `npm run build`: clean.
- `lib.rs`: 5712 → 5581 lines (−131 net; 132 sed-deleted, 8 mod/use lines added).
- ZERO-line byte-parity diff vs v0.3.33 (commit e296d89).

## [v0.3.33] - 2026-05-02

Pure refactor — no behavior change. Continues migration step 5 by extracting PATH-resolution helpers for child command spawn.

### Changed (extracted to `src-tauri/src/command_path.rs`, ~107 lines with doc header, 3 functions)
- `pub(crate) fn resolve_command` — locates a CLI by name on the effective PATH (absolute and relative paths bypass the search).
- `fn command_candidate_paths` — Windows: expands bare stem into `[<command>.exe, .cmd, .bat, .ps1, <command>]`; POSIX: returns unchanged.
- `pub(crate) fn command_search_dirs` — assembles process PATH + Windows install locations, deduplicated case-insensitively.

### Stayed in `lib.rs`
- The spawn machinery (`run_resolved_command_with_timeout`, `run_resolved_command_observed`, `read_pipe_to_end_counting_classified`, `classify_pipe_error`, `resolved_command_builder`, `apply_editorial_agent_environment`, `command_check`) — tightly coupled with `CommandProgressContext` / `TimedCommandOutput` / `log_editorial_agent_*`. Planned for a follow-up batch with editorial orchestration extraction.

### Validation
- `cargo test`: 74 passed.
- `npm run typecheck` + `npm run build`: clean.
- `lib.rs`: 5783 → 5712 lines (−71 net; 73 sed-deleted).
- ZERO-line byte-parity diff vs v0.3.32 (commit e149e9c).

## [v0.3.32] - 2026-05-02

Behavior fix (operator-reported B20): on resume, time and cost caps must NOT be carried forward from the previous session.

### Fixed (B20)
Operator report (2026-05-02): "Encontrei um comportamento indesejado do maestro-app. Quando a sessão é retomada, ele traz de volta o tempo e o custo configurados na sessão anterior. Cada nova sessão, mesmo que seja sessão retomada, deve ser livre para que o usuário defina novos valores ou não. Tempo e custo de sessão não devem ser carregados de sessões passadas."

Root cause: the v0.3.18 B17 fix (cold-open peer pre-population from saved contract) bundled cost/minutes pre-population alongside peer pre-population in `startResumeSession`. Operator's intent was peer continuity only; cost/minutes carry-over was an unintended side-effect.

Fix in `src/App.tsx::startResumeSession`:
- Removed `setMaxSessionMinutes(...)` and `setMaxSessionCostUsd(...)` calls that read from `session.saved_max_session_minutes` / `session.saved_max_session_cost_usd`.
- Removed cap injection from saved contract into `resumeRunOptions`.
- Now reads cost/minutes from `currentSessionRunOptions()` (the operator's current UI input), so resume respects whatever the operator has typed (empty = unlimited).
- Updated `session.resume.contract_applied` NDJSON payload to log `requested_max_session_cost_usd` / `requested_max_session_minutes` (current-UI values) instead of `saved_max_session_cost_usd` / `saved_max_session_minutes`.

### Stayed
- Peer continuity (active_agents, initial_agent) still pre-populated from saved contract — that was the intended B17 behavior and is preserved.
- The Rust-side `ResumableSessionInfo` struct still exposes `saved_max_session_cost_usd` and `saved_max_session_minutes` for inspection (no breaking change to the Tauri command shape).

### Validation
- `npm run typecheck`: clean.
- `npm run build`: clean (1.17s, 2434 modules).
- `cargo test`: 74 passed (no Rust-side change in this batch).

## [v0.3.31] - 2026-05-02

Pure refactor — no behavior change. Continues migration step 5 by extracting public-URL extraction + audit (HEAD/GET probe + private-network IP blocklist) into a dedicated module.

### Changed (extracted to `src-tauri/src/link_audit.rs`, ~232 lines with doc header, 9 functions)
- `pub(crate) fn run_link_audit` — top-level entry with HTTP client + per-URL probe.
- `pub(crate) fn extract_public_urls` + `pub(crate) fn is_public_http_url`.
- `fn is_blocked_link_audit_ip`, `is_blocked_link_audit_ipv4`, `is_blocked_link_audit_ipv6` — RFC 1918/6890/4193/5737/6598 + IPv6 reserved/link-local/ULA/multicast filters.
- `fn probe_public_url`, `probe_public_url_with_get`, `link_audit_row`.

### `pub(crate)` visibility upgrades in `lib.rs`
- `pub(crate) struct LinkAuditRequest` + 1 field, `LinkAuditRow` + 3 fields, `LinkAuditResult` + 5 fields.

### Other changes
- `session_evidence.rs`: rerouted `is_public_http_url` import from `crate::is_public_http_url` to `crate::link_audit::is_public_http_url`.
- Cleaned up 3 unused imports in lib.rs (`reqwest::{blocking::Client, redirect::Policy, Url}`, `std::net::{IpAddr, Ipv4Addr, Ipv6Addr}`).

### Validation
- `cargo test`: 74 passed.
- `npm run typecheck` + `npm run build`: clean.
- `lib.rs`: 5973 → 5783 lines (−190 net; 193 deleted, 3 mod/use lines added).
- ZERO-line byte-parity diff vs v0.3.30 (commit 91aa863).

## [v0.3.30] - 2026-05-02

Pure refactor — no behavior change. Continues migration step 5 by extracting AI provider credential probes (4 providers + helpers) into a dedicated module.

### Changed (extracted to `src-tauri/src/ai_probes.rs`, ~205 lines with doc header, 8 functions)
- `pub(crate) fn run_ai_provider_probe` — top-level entry that builds the HTTP client once.
- 4 per-provider probes: `probe_openai_api`, `probe_anthropic_api`, `probe_gemini_api`, `probe_deepseek_api`.
- Internal helpers: `missing_provider_key_row`, `summarize_ai_probe_response`, `ai_probe_row`.

### `pub(crate)` visibility upgrades in `lib.rs`
- `pub(crate) struct AiProviderProbeRow` + 3 fields.
- `pub(crate) struct AiProviderProbeResult` + 2 fields.

### Validation
- `cargo test`: 74 passed.
- `npm run typecheck` + `npm run build`: clean.
- `lib.rs`: 6137 → 5973 lines (−164 deleted via 3 sed ranges: `'3916,3926d;3828,3887d;3720,3812d'`).
- ZERO-line byte-parity diff vs v0.3.29 (commit fd77a4c) on all 3 ranges (`exit=0` × 3).

## [v0.3.29] - 2026-05-02

Pure refactor — no behavior change. Continues migration step 5 by extracting the resumable-session inspection + agent-runs/* artifact reading helpers into a dedicated module.

### Changed (extracted to `src-tauri/src/session_artifacts.rs`, ~330 lines with doc header, 9 functions)
- `pub(crate) fn inspect_resumable_session_dir` — top-level entry that decides whether a session directory is resumable and enriches the result with saved-contract defaults.
- `pub(crate) fn load_resume_session_state` — reads the latest draft + existing agent results so the orchestrator can pick up mid-session.
- `pub(crate) fn find_latest_draft_artifact` + private `find_latest_draft_artifact_from_artifacts`, `artifact_resume_rank`.
- `pub(crate) fn load_agent_results_from_dir`, `read_agent_artifacts` — recover the per-round agent result vector.
- `pub(crate) fn parse_agent_artifact_name`, `parse_agent_artifact_result` — parse `round-NNN-{peer}-{role}.md` filenames + bullet-list metadata.

### `pub(crate)` visibility upgrades in `lib.rs`
- `pub(crate) struct ResumableSessionInfo` + 16 fields.
- `pub(crate) struct ResumeSessionState` + 4 fields.

### Validation
- `cargo test`: 74 passed.
- `npm run typecheck` + `npm run build`: clean.
- `lib.rs`: 6398 → 6137 lines (−261 deleted via `'2961,3121d;2844,2951d'`; the SessionArtifact struct stays in lib.rs because both `session_artifacts.rs` and `session_resume.rs` consume it).
- ZERO-line byte-parity diff vs v0.3.28 (commit 5f35960) on both ranges.

## [v0.3.28] - 2026-05-02

Pure refactor — no behavior change. Continues migration step 5 by extracting session-time helpers, extract/parse utilities, and protocol-backup helpers into a dedicated module.

### Changed (extracted to `src-tauri/src/session_resume.rs`, ~250 lines with doc header)
- `pub(crate) fn parse_created_at`, `remaining_session_duration`, `session_time_exhausted` — wall-clock helpers around the optional `max_session_minutes` cap.
- `pub(crate) fn extract_bullet_code_value`, `humanize_agent_name` — markdown-bullet parser + agent-key prettifier.
- `pub(crate) fn extract_saved_session_name`, `extract_saved_initial_agent`, `extract_saved_prompt` — parse fields back out of saved `prompt.md`.
- `pub(crate) fn stable_text_fingerprint` — FNV-64 hash for stable per-prompt identifiers.
- `pub(crate) fn count_known_session_markdown_artifacts`, `known_session_activity_unix` — session-directory inspection helpers.
- `pub(crate) struct ProtocolBackupStats` + `pub(crate) fn protocol_backup_stats`, `is_protocol_backup_file_name`, `system_time_to_unix` — `protocolo-anterior-*.md` summary.

### `pub(crate)` visibility upgrades in `lib.rs`
- `pub(crate) struct SessionArtifact` + 4 fields (`round`, `agent`, `role`, `path`).

### Validation
- `cargo test`: 74 passed.
- `npm run typecheck` + `npm run build`: clean.
- `lib.rs`: 6609 → 6398 lines (−211 deleted; 2 ranges: 2836-2860 + 3141-3326). Cleaned up 3 unused imports (`DateTime`, `SystemTime`/`UNIX_EPOCH`, `is_safe_data_file_name`) that no longer have callers in lib.rs.

## [v0.3.27] - 2026-05-02

Pure refactor — no behavior change. Continues migration step 5 by extracting session contract + cost ledger persistence helpers into a dedicated module.

### Changed (extracted to `src-tauri/src/session_persistence.rs`, ~128 lines with doc header)
- `pub(crate) fn session_contract_path`, `cost_ledger_path` — canonical paths inside a session directory.
- `pub(crate) fn load_session_contract`, `write_session_contract` — JSON persistence with parse-failure logging.
- `pub(crate) fn load_cost_ledger`, `write_cost_ledger` — JSON persistence for the cumulative per-session cost ledger.
- `pub(crate) fn append_agent_cost_to_ledger` — appends one entry and recomputes the total.
- `pub(crate) fn api_provider_from_cli` — peer CLI name → provider id mapping.

### `pub(crate)` visibility upgrades in `lib.rs`
- `pub(crate) struct CostLedger` field upgrades (`schema_version`, `run_id`, `entries` had been private; v0.3.27 makes them `pub(crate)` along with the existing `total_observed_cost_usd`).
- `pub(crate) struct CostLedgerEntry` + 9 fields.

### Validation
- `cargo test`: 74 passed.
- `npm run typecheck` + `npm run build`: clean.
- `lib.rs`: 6693 → 6605 lines (−88 deleted; +5 mod+use lines = net −83).
- ZERO-line diff vs v0.3.26 baseline (commit aaaacff).

## [v0.3.26] - 2026-05-02

Pure refactor — no behavior change. Continues migration step 5 by extracting agent input preparation, the active-agents log context builder, and the time-budget anchor helper into a dedicated module.

### Changed (extracted to `src-tauri/src/editorial_inputs.rs`, ~190 lines with doc header)
- `pub(crate) fn effective_agent_input` — gemini-aware adapter that places the prepared prompt into argv when a sidecar input file is written.
- `pub(crate) fn prepare_agent_input` — write large prompts (> 48k chars) to a `<output>-input.md` sidecar.
- `pub(crate) fn build_active_agents_resolved_log_context` — JSON payload builder for `session.editorial.active_agents_resolved` NDJSON entry.
- `pub(crate) fn resolve_time_budget_anchor` — clock anchor for `max_session_minutes` cap (B18 fix from v0.3.18).

### `pub(crate)` visibility upgrades in `lib.rs`
- `pub(crate) struct PreparedAgentInput` + 3 fields.
- `pub(crate) struct EffectiveAgentInput` + 4 fields.

### Validation
- `cargo test`: 74 passed, 0 failed.
- `npm run typecheck` + `npm run build`: clean.
- `lib.rs`: 6840 → 6688 lines (−152; 154 deleted, 4 mod+use added, 2 struct prefix net).

## [v0.3.25] - 2026-05-02

Pure refactor — no behavior change. Continues migration step 5 by extracting the agent specs (CLI args + spec table) and prompt builders (draft/review/revision) into a dedicated module. The heavy `run_editorial_session_inner` block stays in lib.rs for v0.3.26.

### Changed (extracted to `src-tauri/src/editorial_prompts.rs`, ~290 lines with doc header)
- `pub(crate) fn claude_args`, `codex_args`, `gemini_args`, `deepseek_args` — argv templates per peer CLI.
- `pub(crate) fn editorial_agent_specs` — 4-entry vector keyed by peer name.
- `pub(crate) fn resolve_initial_agent_key` — normalize operator's free-form initial-agent string.
- `pub(crate) fn ordered_editorial_agent_specs` — places chosen first key at the head of the spec list.
- `pub(crate) fn build_draft_prompt`, `build_review_prompt`, `build_revision_prompt` — markdown prompt templates.

### `pub(crate)` visibility upgrades in `lib.rs` (consumed by `editorial_prompts.rs`)
- `pub(crate) struct EditorialSessionRequest` + 12 fields (run_id, session_name, prompt, protocol_*, initial_agent, active_agents, max_session_*, attachments, links).
- `pub(crate) struct EditorialAgentSpec` `name`/`command`/`args` fields (the struct itself was already pub(crate); only the `key` field had been upgraded prior).

### Validation
- `cargo test`: 74 passed, 0 failed.
- `npm run typecheck` + `npm run build`: clean.
- `lib.rs`: 7081 → 6840 lines (−241; net delta 6836+4 mod/use lines).

## [v0.3.24] - 2026-05-02

Pure refactor — no behavior change. Begins migration step 5 ("editorial orchestration and artifacts") by extracting the small/standalone helpers cluster (active-agent filtering/resolution, review-complaint fingerprinting, RUNNING-artifact finalization, per-attempt running/error artifact writers) into a dedicated module. The heavy `run_editorial_session_inner` block stays in lib.rs for a follow-up batch.

### Changed (extracted to `src-tauri/src/editorial_helpers.rs`, ~250 lines with doc header)
- `pub(crate) fn filter_existing_agents_to_active_set` — resume-side filter mirroring `normalize_active_agents` alias normalization.
- `pub(crate) fn resolve_effective_active_agents` — request/saved/default decision tree with audit-log source label.
- `pub(crate) fn review_complaint_fingerprint` — stable u64 hash for persistent-divergence detection.
- `pub(crate) struct FinalizeRunningArtifactsGuard` (+ impl + Drop) — RAII guard for RUNNING-placeholder cleanup on every exit path (Codex NB-2 from v0.3.15).
- `pub(crate) fn finalize_running_agent_artifacts` — idempotent final-pass safety net.
- `pub(crate) fn write_editorial_agent_running_artifact` — initial RUNNING placeholder before child spawn.
- `pub(crate) fn write_editorial_agent_error_artifact` — error envelope for failed commands without structured output.

### `pub(crate)` visibility upgrades in `lib.rs` (consumed by `editorial_helpers.rs`)
- `pub(crate) fn read_text_file` (consumed by `finalize_running_agent_artifacts`).
- `pub(crate) fn extract_stdout_block` (consumed by `review_complaint_fingerprint`).

### Validation
- `cargo test`: 74 passed, 0 failed (zero regression).
- `npm run typecheck`: clean.
- `npm run build`: clean (PostEditor chunk-size warning is pre-existing).
- `lib.rs`: 7270 → 7075 lines (−195). `editorial_helpers.rs`: ~250 lines new.

### Operational notes
- Followed the now-stable "delete first, edit second" sequence: built `editorial_helpers.rs`, captured fresh line numbers, single `sed -i 4304,4498d` (195 lines), then added `mod` + `use` + `pub(crate)` upgrades.
- Defensive `#[allow(clippy::too_many_arguments)]` added on `write_editorial_agent_running_artifact` (9 args) and `write_editorial_agent_error_artifact` (11 args), following the v0.3.22 sibling-helper convention.

## [v0.3.23] - 2026-05-02

Pure refactor — no behavior change. Begins `docs/code-split-plan.md` migration step 4 ("Cloudflare D1 and Secrets Store operations") by extracting the Cloudflare client + probe + D1 + Secrets Store surface into a dedicated module.

### Changed (extracted to `src-tauri/src/cloudflare.rs`, ~960 lines with doc header)
- HTTP layer: `cloudflare_client`, `cloudflare_get`, `cloudflare_post_json`, `cloudflare_patch_json`, `cloudflare_get_paginated_results`, `cloudflare_page_path`, `cloudflare_verify_path`, `cloudflare_token_kind`, `cloudflare_error_summary`.
- Token resolution: `token_from_probe_request`, `token_source_label`, `cloudflare_token_from_provider_request`.
- JSON helpers: `cloudflare_result_names`, `cloudflare_result_id_for_name`, `cloudflare_store_records`, `cloudflare_store_for_target_or_existing`, `cloudflare_secret_ids_by_name`, `cloudflare_secret_id_from_response`, `cloudflare_created_result_id`, `CloudflareStoreRecord` struct.
- D1 + Secrets Store ensure logic: `ensure_cloudflare_d1_database`, `ensure_cloudflare_secret_store`, `provision_maestro_d1_schema`, `link_secret_store_reference`.
- AI provider bridge: `ai_provider_secret_values`, `upsert_ai_provider_secrets`, `write_ai_provider_metadata_to_cloudflare`.
- Probe entry point: `run_cloudflare_probe`, `probe_row`.

### `pub(crate)` visibility upgrades in `lib.rs` (consumed by `cloudflare.rs`)
- `pub(crate) fn env_value_with_scope` (used by token resolvers in cloudflare.rs).
- `pub(crate) struct CloudflareProbeRequest` + 6 fields.
- `pub(crate) struct CloudflareProbeRow` + 3 fields.
- `pub(crate) struct CloudflareProbeResult` + 1 field.
- `pub(crate) struct CloudflareProviderStorageRequest` + 5 fields.

### Stayed in `lib.rs`
- `cloudflare_env_snapshot`, `verify_cloudflare_credentials` (Tauri commands — must stay because of `tauri::generate_handler!` registration).
- `persist_ai_provider_cloudflare_marker`, `persist_ai_provider_config_to_cloudflare`, `enrich_ai_provider_config_from_cloudflare`, `read_ai_provider_cloudflare_metadata` (AI provider <-> Cloudflare bridges).
- `tests::secrets_store_selection_*`, `tests::routes_*_tokens_to_*_verify_endpoint`, `tests::ai_provider_secret_values_use_cloudflare_safe_names` (kept in `lib.rs::tests`; functions re-imported via `use crate::cloudflare::{cloudflare_page_path, cloudflare_store_for_target_or_existing, cloudflare_verify_path}`).

### Validation
- `cargo test`: 74 passed, 0 failed (zero regression).
- `npm run typecheck`: clean.
- `npm run build`: clean, 1.17s, 2434 modules transformed (PostEditor chunk-size warning is pre-existing).
- `lib.rs`: 8236 → 7260 lines (−976). `cloudflare.rs`: ~960 lines new.

### Operational notes
- Followed advisor's "delete first, edit second" sequence: built `cloudflare.rs` before any `lib.rs` mutation, then captured fresh line numbers immediately before a single `sed -i 5250,6225d` deletion (976 lines), then added `mod` + `use` + `pub(crate)` upgrades. Same workflow that made v0.3.22 succeed in 2 cross-review rounds vs v0.3.21's 5.

## [v0.3.22] - 2026-05-02

Pure refactor — no behavior change. Continues `docs/code-split-plan.md` migration step 3 by bundling the 3 isomorphic provider runners (OpenAI / Anthropic / Gemini) and their shared helper family into a single new module. v0.3.21 already extracted DeepSeek (the structural outlier). With v0.3.22, all 4 per-provider runners are out of `lib.rs`.

### Changed (extracted to `src-tauri/src/provider_runners.rs`, ~1100 lines with doc header)
- **Runners** (3 isomorphic): `pub(crate) fn run_openai_api_agent`, `pub(crate) fn run_anthropic_api_agent`, `pub(crate) fn run_gemini_api_agent`. Each preserves its provider-specific request body (responses-API for OpenAI; messages-API for Anthropic; generateContent for Gemini), response parser, and endpoint string byte-for-byte.
- **Unified provider helper family**: `pub(crate) fn editorial_api_system_prompt`, `pub(crate) fn api_cost_preflight_result`, `pub(crate) fn write_provider_missing_key_result`, `pub(crate) fn write_provider_error_result`, `pub(crate) fn write_provider_failure_result`, `pub(crate) fn write_provider_success_result`, `pub(crate) fn log_provider_api_started`.
- **Per-provider model resolvers**: `pub(crate) fn resolve_openai_model`, `pub(crate) fn resolve_anthropic_model`, `pub(crate) fn resolve_gemini_model`. Plus the private helpers they share (`choose_preferred_model`, `api_model_ids`, `gemini_model_ids`).
- **Per-provider response parsers**: `pub(crate) fn openai_response_text`, plus the private `anthropic_response_text`, `gemini_response_text`, `gemini_usage_tokens`.

### `pub(crate)` visibility upgrades in `lib.rs` (consumed from `provider_runners.rs`)
- `fn provider_label_for_agent`, `fn provider_remote_present`, `fn provider_key_for_agent` — the last has an external caller in `should_run_agent_via_api`.
- `fn api_input_estimate_chars` — also has unit tests in `lib.rs::tests`.
- `fn openai_api_input`, `fn anthropic_api_user_content`, `fn gemini_api_user_parts` — all 3 have unit tests in `lib.rs::tests` covering the JSON envelope shape per provider.

### Stayed in `lib.rs` (not moved this batch)
- The 4 attachment-shape predicates (`provider_supports_native_attachment`, `openai_api_attachment_supported`, `openai_api_file_attachment_supported`, `anthropic_api_attachment_supported`, `gemini_api_attachment_supported`, `attachment_within_native_payload_cap`) — they sit beside the `*_api_input` builders that own the tests. Moving them later if the test ownership migrates first.
- The 3 attachment-payload builders (`openai_api_input`, `anthropic_api_user_content`, `gemini_api_user_parts`) — same reason.
- `provider_label_for_agent`/`provider_remote_present`/`provider_key_for_agent` — shared by other lib.rs code paths beyond the runners.

### Validation
- `cargo test`: 74 passed, 0 failed (zero regression).
- `npm run typecheck`: clean.
- `npm run build`: clean, 1.48s, 2434 modules transformed (PostEditor chunk-size warning is pre-existing).
- `lib.rs`: 9351 → 8233 lines (−1118). `provider_runners.rs`: ~1100 lines new.

### Operational notes
- Followed the advisor's "delete first, edit second" sequence: built `provider_runners.rs` before any `lib.rs` mutation, then captured fresh line numbers immediately before a single `sed -i` deletion (range `3886,5003d`), then added `mod` + `use` + `pub(crate)` upgrades. This eliminated the line-shift class that bit v0.3.21.
- `FUNDING.yml` URL update (operator-requested) is bundled into the same commit since it is a small documentation-only follow-up that was deferred from v0.3.21.

## [v0.3.21] - 2026-05-02

Pure refactor — no behavior change. Continues `docs/code-split-plan.md` migration order step 3 ("AI provider credentials/probes, including DeepSeek"). v0.3.20 extracted the shared `provider_retry` primitives; v0.3.21 begins migrating the per-provider runners themselves, starting with DeepSeek because it is the structural outlier (custom error helper, predates the unified `write_provider_failure_result` family used by openai/anthropic/gemini). The remaining 3 isomorphic runners + their shared helper family are scheduled for v0.3.22.

### Changed (extracted to `src-tauri/src/provider_deepseek.rs`, 467 lines with doc header)
- `pub(crate) fn run_deepseek_api_agent(...) -> EditorialAgentResult` — the chat-completions runner with retry, cost pre-flight, output sanitization and NDJSON instrumentation. Byte-for-byte preserves every status string, log line, format string and tone classification from the v0.3.20 lib.rs source.
- `pub(crate) fn write_deepseek_error_result(...)` — DeepSeek-specific error artifact + result envelope (custom helper, not the unified `write_provider_failure_result`).
- `pub(crate) fn deepseek_model() -> String` — env override (`MAESTRO_DEEPSEEK_MODEL` / `CROSS_REVIEW_DEEPSEEK_MODEL`) with `deepseek-v4-pro` fallback.
- `pub(crate) fn resolve_deepseek_model(client, api_key) -> String` — env override first, then `/models` listing with the candidate-preference list, with `deepseek-reasoner` as ultimate fallback.
- `pub(crate) fn deepseek_model_ids(value) -> Vec<String>` — JSON ID extractor.
- `#[cfg(test)] fn deepseek_model_ids_extract_current_api_shape` test moved with the function it covers.

### `pub(crate)` visibility upgrades in `lib.rs` (consumed from `provider_deepseek.rs`)
- `struct AiProviderConfig` + the 4 fields the runner reads (`deepseek_api_key`, `deepseek_api_key_remote`, plus the parallel openai/anthropic/gemini fields upgraded for consistency since v0.3.22 will need them).
- `struct EditorialAgentResult` + all 12 fields (since the runner constructs the struct).
- `fn first_env_value` (consumed by `deepseek_model` and `resolve_deepseek_model`).
- `fn effective_provider_key` (consumed by the runner).
- `fn log_editorial_agent_finished` (consumed by the runner and the error helper).
- `fn extract_maestro_status` (consumed by the runner).
- `fn api_error_message` (consumed by the runner).

### Stayed in `lib.rs` (move in v0.3.22)
- `run_openai_api_agent`, `run_anthropic_api_agent`, `run_gemini_api_agent` (the 3 isomorphic runners that share the unified provider helper family).
- `editorial_api_system_prompt`, `api_cost_preflight_result`, `write_provider_missing_key_result`, `write_provider_error_result`, `write_provider_failure_result`, `api_input_estimate_chars`, `provider_key_for_agent`, `provider_remote_present`, `openai_response_text`, `log_provider_api_started`, `write_provider_success_result` — the unified provider helpers.

### Validation
- `cargo test`: 74 passed, 0 failed (zero regression). The `deepseek_model_ids_extract_current_api_shape` test now runs as `provider_deepseek::tests::deepseek_model_ids_extract_current_api_shape`.
- `npm run typecheck`: clean.
- `npm run build`: clean, 1.11s, 2434 modules transformed (PostEditor chunk-size warning is pre-existing).
- `lib.rs`: 9776 → 9351 lines (−425). `provider_deepseek.rs`: 467 lines new (includes 17-line doc header + use block + the test module).

## [v0.3.20] - 2026-05-02

Pure refactor — no behavior change. Extracts the shared provider HTTP networking primitives into `src-tauri/src/provider_retry.rs` (137 lines with doc comments). Per `docs/code-split-plan.md` migration step 3 ("AI provider credentials/probes"). The 4 provider runner functions (DeepSeek/OpenAI/Anthropic/Gemini) stay in `lib.rs` and will move in v0.3.21+ along with their request body shapes and response parsers.

### Changed (extracted to `src-tauri/src/provider_retry.rs`)
- `pub(crate) fn build_api_client(timeout: Option<Duration>) -> Result<Client, reqwest::Error>` — `reqwest::blocking::Client` factory with the Maestro user-agent.
- `pub(crate) const PROVIDER_RETRY_MAX_ATTEMPTS: u32 = 2` — at most one retry.
- `pub(crate) const PROVIDER_RETRY_NETWORK_BACKOFF_MS: u64 = 1500` — backoff between attempts on transient network errors.
- `pub(crate) const PROVIDER_RETRY_429_DEFAULT_SECS: u64 = 30` — default sleep on 429 with no Retry-After header.
- `pub(crate) const PROVIDER_RETRY_429_CAP_SECS: u64 = 120` — hard cap on Retry-After to prevent provider-driven hangs.
- `pub(crate) fn parse_retry_after_header` — RFC 7231 delta-seconds OR RFC 2822 HTTP-date.
- `pub(crate) fn send_with_retry<F>` — bounded retry policy on transient network errors and HTTP 429 with Retry-After respect; logs `session.provider.retry_network` and `session.provider.retry_after_429` warn entries via the v0.3.19 `logging::write_log_record`.

### Stayed in `lib.rs`
- `run_deepseek_api_agent`, `run_openai_api_agent`, `run_anthropic_api_agent`, `run_gemini_api_agent` (the 4 runners) — each with provider-specific request body shapes and response parsers; planned for v0.3.21+ batches.
- `editorial_api_system_prompt`, `api_cost_preflight_result`, `write_provider_missing_key_result`, `write_provider_error_result`, `write_deepseek_error_result`, `api_input_estimate_chars`, `provider_key_for_agent`, `provider_remote_present`, `openai_response_text` — provider-orchestration helpers; planned for the same v0.3.21+ batches.

### Validation
- `cargo test`: 74 passed (zero regression). Tests for `parse_retry_after_header_*` (4) re-imported via `mod tests` `use crate::provider_retry::parse_retry_after_header;`.
- `npm run typecheck` + `npm run build`: clean.
- `cargo clippy --no-deps --all-targets`: 0 errors.
- `lib.rs`: 9873 → 9776 lines (−97). `provider_retry.rs`: 137 lines new.

## [v0.3.19] - 2026-05-02

Pure refactor — no behavior change. Continues `docs/code-split-plan.md` migration order step 2 ("logging and path safety"), which was partially completed in v0.3.17. This batch finishes the logger surface extraction.

### Changed (extracted to `src-tauri/src/logging.rs`, 157 lines with doc comments)
- `NATIVE_LOG_SEQUENCE: AtomicU64` static (process-scoped sequence stamp).
- `LogSession` struct (`#[derive(Clone)]`, holds `id`, `path`, `Arc<Mutex<()>> write_lock`).
- `LogEventInput` struct (`#[derive(Deserialize)]`).
- `LogWriteResult` struct (`#[derive(Serialize)]`).
- `create_log_session()` factory.
- `write_log_record(&LogSession, LogEventInput) -> Result<LogWriteResult, String>`.

### Stayed in `lib.rs`
- `runtime_profile` and `write_log_event` Tauri commands (the IPC boundary uses `tauri::State<LogSession>` and stays in the `#[tauri::command]` registry).
- `install_process_panic_hook()` and `write_early_crash_record()` (compose a separate JSON crash schema and integrate with process-level panic state).
- `log_editorial_agent_finished/spawned/running` helpers (depend on `EditorialAgentResult` and move with the editorial orchestration batch v0.3.22).
- Sanitization helpers (`sanitize_text`, `sanitize_short`, `sanitize_value`, `redact_secrets`, `truncate_text_head_tail`, `stable_text_fingerprint`) — planned for a separate `text_utils.rs` extraction.

### Visibility upgrades (required for cross-module access)
- `sanitize_short` and `sanitize_value` upgraded from `fn` to `pub(crate) fn` so `logging.rs` can import them. `sanitize_text` was already `pub(crate)`. No body changes.

### Validation
- `cargo test`: 74 passed (no test count change; same surface).
- `npm run typecheck` clean.
- `npm run build` clean.
- `cargo clippy --no-deps --all-targets`: 0 errors. `#![deny(clippy::disallowed_methods)]` from v0.3.16 preserved.
- `lib.rs`: 9930 → 9873 lines (−57). `logging.rs`: 157 lines new.

### Next batches per `docs/code-split-plan.md`
- v0.3.20: extract editorial provider runners (DeepSeek/OpenAI/Anthropic/Gemini API + retry helper) into `providers/` module.
- v0.3.21: extract Cloudflare D1 + Secrets Store operations.
- v0.3.22: extract editorial orchestration (`run_editorial_session_inner`, agent helpers, build_session_minutes, etc.).

## [v0.3.18] - 2026-05-02

Closes the 3 production bugs found in `data/logs/maestro-2026-05-01T18-56-07Z` and `19-01-53Z` running v0.3.16/v0.3.17. These were deferred from v0.3.17 per `docs/code-split-plan.md` rule "Do not combine code split with behavior changes."

### Fixed
- **B17 — saved_contract pre-select on cold-open.** v0.3.16 fix made the frontend resume invoke always send the React state. But the React state defaults to all-4 peers on cold app open, so an operator who paused with `["deepseek"]` and reopened the app saw all 4 peers re-spawn. Backend NDJSON 19:02:45 confirmed `saved_contract: ["deepseek"]`, `requested: [all 4]`, `source: "request"`. Fix: extended `ResumableSessionInfo` to expose `saved_active_agents`, `saved_initial_agent`, `saved_max_session_cost_usd`, `saved_max_session_minutes` from the inspected contract; `startResumeSession` in `App.tsx` now applies those to React state (`setActiveAgents`, `setInitialAgent`, `setMaxSessionMinutes`, `setMaxSessionCostUsd`) AND snapshots them into the runOptions passed to `runRealEditorialSession`, so the resume request reflects what the operator paused with — not the cold-open default. New NDJSON `session.resume.contract_applied` log entry surfaces the applied snapshot for audit.
- **B18 — TIME_LIMIT_REACHED instant-fire on resume.** Operator set `max_session_minutes=5` intending to limit the resume to 5 minutes; backend computed elapsed = (now − 2026-04-26T19:28:26) ≈ 5 days ≫ 5 min and exhausted in 130ms without spawning anything. New helper `resolve_time_budget_anchor(created_at, is_resume, now) -> DateTime<Utc>` returns `now` when resuming and `created_at` otherwise. The 7 call sites of `session_time_exhausted` and `remaining_session_duration` in `run_editorial_session_inner` swap from `created_at` to `time_budget_anchor`. `created_at` remains the single source of truth for cumulative metrics and stays persisted in the session contract; only the time-budget gate switches to the per-call anchor.
- **B19 — stale "Ultimo estado" mixing artifacts from prior runs.** When a resumed session loaded `existing_agents` from `agent-runs/`, the in-memory list included peers that ran in EARLIER sessions but were narrowed out for THIS run. Frontend "Ultimo estado" summary then showed all 4 peers as if they had run, misleading the operator. New helper `filter_existing_agents_to_active_set(existing, active_agent_keys)` filters the recovered list to only include peers in this run's effective active set, normalizing aliases (`"Anthropic"`/`"Claude"` → `"claude"`, `"OpenAI"`/`"Codex"` → `"codex"`, etc.) to mirror `normalize_active_agents`. Older artifacts stay on disk under `agent-runs/`; only the in-memory snapshot used for status reporting is filtered.

### Added (testable helpers from B18/B19)
- `resolve_time_budget_anchor(created_at, is_resume, now)` — pure function; takes `now` as a parameter for deterministic testing.
- `filter_existing_agents_to_active_set(existing, active_agent_keys)` — pure function; alias normalization mirrors `normalize_active_agents`.

### Backend `ResumableSessionInfo` extensions (B17)
4 new fields populated by `inspect_resumable_session_dir` from the saved session contract:
- `saved_active_agents: Vec<String>`
- `saved_initial_agent: Option<String>` (filtered to non-empty)
- `saved_max_session_cost_usd: Option<f64>`
- `saved_max_session_minutes: Option<u64>`

### Frontend (B17)
- `ResumableSessionInfo` TypeScript type extended with the 4 fields.
- `startResumeSession`:
  - Validates `saved_active_agents` against `initialAgentOptions` keys; falls back to current React state if invalid.
  - Calls `setActiveAgents`, `setInitialAgent`, `setMaxSessionMinutes`, `setMaxSessionCostUsd` to keep UI in sync with the saved contract.
  - Builds `resumeRunOptions` synchronously from the validated saved contract values, bypassing the default React state.
  - Emits `session.resume.contract_applied` info NDJSON log entry with the applied snapshot.

### Validation
- `cargo test`: 73 passed (7 new + 66 existing). New: `resolve_time_budget_anchor_returns_now_when_resuming`, `resolve_time_budget_anchor_returns_created_at_when_fresh_start`, `filter_existing_agents_keeps_only_agents_in_active_set`, `filter_existing_agents_normalizes_agent_name_aliases`, `filter_existing_agents_returns_empty_when_active_set_is_empty`, `inspect_resumable_session_dir_reports_saved_active_agents_for_picker`, `inspect_resumable_session_dir_returns_empty_saved_when_contract_missing`.
- `npm run typecheck` clean.
- `npm run build` clean (~1s, 2434 modules).
- `cargo clippy --no-deps --all-targets`: 0 errors. v0.3.16's `#![deny(clippy::disallowed_methods)]` preserved.

### Notes
- `created_at` continues to anchor cumulative metrics (artifact age, cost ledger entries) and stays in the session contract.
- The 4 new `saved_*` fields in `ResumableSessionInfo` are additive; no existing field renamed or removed.
- `session.resume.contract_applied` is a new info-level NDJSON event; existing `session.editorial.active_agents_resolved` already logs the post-normalize effective values, so the new event is for "what the picker applied", complementing the existing "what the runtime resolved".

## [v0.3.17] - 2026-05-02

Pure refactor — no behavior change. Continues `docs/code-split-plan.md` migration order step 2 ("logging and path safety"). `lib.rs` is at 9759 lines pre-split; this batch trims it by 127 lines into a new `src-tauri/src/app_paths.rs` module (220 lines with doc comments).

### Changed (extracted to `src-tauri/src/app_paths.rs`)
- `APP_ROOT` `OnceLock<PathBuf>` static moved into `app_paths.rs`. `lib.rs::initialize_app_root` now calls `app_paths::try_set_app_root(...)`. Panic-record helper uses `app_paths::app_root_if_initialized()`.
- Path resolution: `app_root`, `resolve_portable_app_root`, `portable_root_from_exe_path`, `data_dir`, `logs_dir`, `human_logs_dir`, `human_log_path_for`, `config_dir`, `bootstrap_config_path`, `ai_provider_config_path`, `sessions_dir`.
- Boot-time path helpers: `early_logs_dir`, `active_or_early_logs_dir` (and new `app_root_if_initialized` returning `Option<PathBuf>` for the panic record).
- Path-safety primitives: `checked_data_child_path`, `is_safe_relative_data_path`, `is_safe_data_file_name`, `safe_run_id_from_entry`, `sanitize_path_segment`.

### Stayed in `lib.rs`
- `initialize_app_root` (touches `&tauri::App`, the runtime boundary).
- `install_process_panic_hook` and `write_early_crash_record` (compose JSON crash records that depend on `Utc`, `serde_json`, and the local `sanitize_text` redactor).
- `create_log_session` and the rest of the diagnostic logger (planned for the next split batch).

### Validation
- `cargo test`: 66 passed (no test count change; same surface — the `mod tests` block re-imports `config_dir`, `portable_root_from_exe_path` from `app_paths` to keep the cargo unused-import warnings clean in non-test builds).
- `npm run typecheck` + `npm run build`: clean.
- `cargo clippy --no-deps --all-targets`: 0 errors. The `#![deny(clippy::disallowed_methods)]` from v0.3.16 is preserved at lib.rs and main.rs.

### Notes
- `lib.rs` line count: 9759 → 9632.
- No public Tauri command name changed. No portable data path changed. No secret redaction logic touched.
- Per `docs/code-split-plan.md` migration order, the next batches will be: AI provider credentials/probes (step 3), Cloudflare D1 + Secrets Store (step 4), editorial orchestration + artifacts (step 5).

## [v0.3.16] - 2026-05-02

Patch release fechando 4 bugs reais surfaceados em logs de produção da v0.3.15 + 2 hardenings (NB-2/NB-5) + clippy hygiene (A+B do parecer pós-v0.3.15). Operator analisou `data/logs/maestro-2026-05-01T18-01-15Z-pid28124.ndjson` rodando v0.3.15 e confirmou: B11 ficou parcialmente casca-vazia + 3 novos bugs (B13/B14/B15). Codex emitiu parecer pedindo NB-2 + NB-5 prioritários.

### Fixed
- **B11 regressão de v0.3.15.** Operator confirmou: "selecionei apenas DeepSeek, mas todos rodaram." Root cause: `App.tsx:1661` (`startResumeSession`) chamava `runRealEditorialSession` com apenas 3 argumentos posicionais — `runOptions` ficava `undefined`, o resume invoke enviava `active_agents: null`, e o backend caía no saved_contract (que tinha os 4 originais). A v0.3.15 fechou o lado backend (`#[serde(default)]`, log `active_agents_resolved`) mas deixou o callsite frontend incompleto. v0.3.16 expande `startResumeSession` para invocar `currentSessionRunOptions()` e propagar `selectedInitialAgent` + `runOptions` de fato. Falha de validação (peers fora dos limites) bloqueia o resume com `setOperation({ status: 'blocked', ... })` em vez de avançar com estado inválido.
- **B13 PROVIDER_NETWORK_ERROR sem retry.** Logs mostravam todos os 4 peers timing out em ~30s sequencialmente, sem retry, perdendo rodadas inteiras a cada blip de rede. Novo helper `send_with_retry(log_session, run_id, provider_label, make_request)` em `lib.rs` envolve cada call site (DeepSeek, OpenAI, Anthropic, Gemini) com 1 retry pós-1.5s backoff em qualquer `Err` de `.send()`. Limita o desperdício a 3s extras por agente em pior caso, recupera transient flakiness em melhor caso. Logs `session.provider.retry_network` warn registram cada retry com `error_is_timeout` / `error_is_connect` flags do reqwest.
- **B14 HTTP 429 sem respeitar Retry-After.** Mesma sessão mostrava Claude retornando 429 às 18:03:10 e Maestro re-tentando 90 segundos depois → 429 novamente. `send_with_retry` agora detecta `response.status() == 429` e dorme por `parse_retry_after_header(headers)` (delta-seconds OU HTTP-date RFC 2822), default 30s, cap 120s. Logs `session.provider.retry_after_429` warn registram a fonte (`header` vs `default`) para auditoria post-hoc.
- **B15 loop infinito em rounds all-error.** Logs mostravam round 74, 75, 76 com todos os 4 peers em tone=error/blocked sem nenhuma escalada. Novo contador `consecutive_all_error_rounds` em `run_editorial_session_inner` incrementa quando `round_results.iter().all(|r| r.tone == "error" || r.tone == "blocked")` e reseta quando ao menos um peer foi ok/warn. Após 3 rounds consecutivos all-error, emite `session.escalation.all_peers_failing` (level=error NDJSON com peer statuses snapshot) e retorna `EditorialSessionResult` com status `ALL_PEERS_FAILING`, pausando a sessão para revisão do operator em vez de queimar quota e tempo.

### Added (Codex NB-2/NB-5 hardenings)
- **NB-2 finalize-running Drop guard.** `FinalizeRunningArtifactsGuard` (RAII) é instanciado no início de `run_editorial_session_inner` com `_finalize_guard = FinalizeRunningArtifactsGuard::new(agent_dir.clone())`. O `Drop` impl chama `finalize_running_agent_artifacts(&self.agent_dir)`, garantindo que `Status: RUNNING` placeholders sejam reescritos para `AGENT_FAILED_NO_OUTPUT` em todos os exit paths — incluindo `?` early returns e panics, que a hook em `editorial_session_result` da v0.3.15 não cobria. `finalize_running_agent_artifacts` permanece idempotente, então o duplo-passo no caminho normal (Drop + chamada explícita em `editorial_session_result`) é no-op no segundo. Teste novo `finalize_running_artifacts_drop_guard_runs_on_panic` usa `std::panic::catch_unwind` para provar que o Drop dispara mesmo em panic mid-session.
- **NB-5 spawn-funnel lint guard.** Novo `src-tauri/clippy.toml` com `disallowed-methods = [{ path = "std::process::Command::new", reason = "..." }]`. `lib.rs` ganha `#![warn(clippy::disallowed_methods)]` no topo. `hidden_command` (a única entrada legítima do funil editorial) e dois test fixtures recebem `#[allow(clippy::disallowed_methods)]` com comentário SAFE-FUNNEL. Qualquer futuro `Command::new` direto em `lib.rs` ou módulos relacionados dispara warn no `cargo clippy` (que já é gate). Substitui também o único `Command::new("xdg-open")` direto restante (linha 712 anterior) por `hidden_command("xdg-open")` para unificar o pattern em todos os OSes.

### Hygiene (advisor recommendation A+B aplicada)
- `manual_pattern_char_comparison` em `lib.rs:6840`: `matches!(char, '.' | ',' | ';' | ':')` colapsado para `['.', ',', ';', ':']` array literal.
- `if_same_then_else` em `lib.rs:5799-5811` (código próprio v0.3.15 do tone derivation B2): `timed_out` e `AGENT_FAILED_EMPTY/AGENT_FAILED_NO_OUTPUT` retornavam ambos "error" em arms separados; merged em uma única condição `||`. Sem perda de legibilidade.
- Clippy warnings: 19 → 17 (2 idiom fixes aplicados; demais 17 são `too_many_arguments` em código autoral de fases anteriores, deferred para o split planejado de `lib.rs` per `docs/code-split-plan.md`).

### Validation
- `cargo test`: **66 passed** (5 new + 61 existing). Novos: `finalize_running_artifacts_drop_guard_runs_on_panic`, `parse_retry_after_header_reads_delta_seconds`, `parse_retry_after_header_reads_http_date`, `parse_retry_after_header_returns_none_when_absent`, `parse_retry_after_header_returns_none_for_garbage`.
- `npm run typecheck`: clean.
- `npm run build`: clean (~1s, 2434 modules).
- `cargo clippy --no-deps`: 17 warnings (todos pré-existentes de `too_many_arguments` ou já anotados com `#[allow(clippy::too_many_arguments)]` em `build_active_agents_resolved_log_context`).

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
