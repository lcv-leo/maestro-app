# Maestro Editorial AI - Architecture Plan v0.1

Status: planning baseline, not implementation.
Date: 2026-04-26.
Primary protocol: Protocolo Editorial v1.10.0.

## 1. Product Definition

Maestro Editorial AI is an independent portable Windows editorial workbench. It is not a GUI over cross-review-mcp and does not depend on cross-review-mcp at runtime. It incorporates the proven operating logic of cross-review-mcp into its own codebase: agent capability probes, explicit rounds, parsed statuses, NEEDS_EVIDENCE discipline, strict unanimity, persisted convergence snapshots, and operator escalation.

The app lives under `maestro-app`, runs from any folder, and uses no installer as a product requirement. Local-first operation persists durable application state as JSON/NDJSON files under its own folder. Operator-approved alternatives can store secrets in Windows user environment variables or move all configuration and secret references to Cloudflare D1 plus Cloudflare Secrets Store.

## 2. Hard Gates

- No text becomes `publicavel` unless the mechanical evidence engine and all active AI agents converge `READY` in the same round.
- Maestro itself is a deterministic fourth peer. Final delivery requires Claude, Codex, and Gemini to converge plus `MaestroPeer: READY`.
- Inviolable unanimity rule: regardless of time, cost, number of rounds, rate limits, or operator impatience, the final text is delivered only after unanimous acceptance. While any divergence remains, the work remains open.
- Partial agreement is non-convergence.
- `NEEDS_EVIDENCE` blocks publication until the requested evidence is supplied or the item is explicitly escalated and the formal state remains below `publicavel`.
- The operator may provide evidence, revise scope, abort, or export a non-publicable draft, but cannot silently convert unresolved blockers into `publicavel`.
- Protocolo Editorial v1.10.0 is the initial planning protocol, but the product must treat editorial protocols as mutable operator-managed documents. Every editorial session pins the active protocol by `protocol_id`, declared version, import timestamp, and content hash at intake. Protocol upgrades are explicit import events with semantic diff.
- No persistent data outside the app folder unless the operator explicitly chooses Windows env-var hybrid persistence or Cloudflare remote persistence.
- Configuration persistence has exactly three modes: local JSON for everything, Windows env-var hybrid for tokens/API keys plus JSON for non-secret settings, or Cloudflare remote persistence using D1 `maestro_db` plus Cloudflare Secrets Store. See `docs/configuration-persistence.md`.
- GitHub synchronization begins from the first implementation, with public release only after maturity. Repository hygiene is therefore a day-zero hard gate: no secrets, API keys, credentials, local session data, raw CLI transcripts, user drafts, evidence caches, or generated exports may be committed.
- Before any future private-to-public repository flip, run a full pre-cloud exposure audit and full-history secret scan.
- Work as if GitHub Secret Scanning, Code Scanning, CodeQL Default Setup, Dependabot alerts, Dependabot version updates, GitHub Releases, GitHub Packages, GitHub Pages, and GitHub Sponsors are already enabled. Repository files must be compatible with these checks from the first commit.
- CodeQL must remain on GitHub Default Setup. Advanced Setup requires prior technical justification and explicit operator authorization.

## 3. Runtime Architecture

Recommended baseline:

- Target OS: Windows 11+ only. Do not dilute the product with Windows 7/8/10 compatibility workarounds.
- Shell: Tauri 2 + WebView2, distributed as a portable folder/executable, not NSIS/MSI.
- UI: React 19 + Vite 8 + TypeScript 6, matching the modern workspace pattern.
- Design system/tooling: lucide-react for icons, Biome for formatting, ESLint where framework-specific lint rules are needed, Vitest for unit/integration tests.
- Core application logic: TypeScript. Tauri necessarily brings Rust for the native shell; keep Rust focused on shell/process/filesystem boundaries, not editorial business logic.
- WebView data directory: explicitly set to `./data/webview` through the Tauri/WebView2 data-directory API.
- All logs, caches, evidence records, drafts, reports, and session files live under `./data`.

If WebView2 is absent, the app fails closed with a clear diagnostic. It must not run an installer silently.

Current baseline verified on 2026-04-26 via npm metadata: `@tauri-apps/cli 2.10.1`, `@tauri-apps/api 2.10.1`, `vite 8.0.10`, `react 19.2.5`, `react-dom 19.2.5`, `typescript 6.0.3`, `vitest 4.1.5`, `@vitejs/plugin-react 6.0.1`, `@biomejs/biome 2.4.13`, `eslint 10.2.1`, `lucide-react 1.11.0`. Scaffold must use latest stable versions at implementation time and pin them in lockfiles.

Local prerequisite recheck observed 2026-04-26: Rust is installed through rustup at `C:\Users\leona\.cargo\bin` with `stable-x86_64-pc-windows-msvc` as the default toolchain. If a running terminal cannot resolve `rustc`, restart the terminal/session or reload the user `PATH`; the persistent user `PATH` already contains `.cargo\bin`.

## 4. Core Modules

- `agent-adapters`: Codex CLI, Claude CLI, Gemini CLI process adapters with model pins, timeout policy, redaction, stdout/stderr capture, JSON/JSONL parser hardening, auth probes, update probes, and no silent model downgrade. See `docs/cli-agent-audit.md`.
- `ai-provider-adapters`: official API/SDK adapters for OpenAI/Codex, Anthropic/Claude, and Google/Gemini with model pins, request budgets, provider request IDs, and transport provenance.
- `credential-manager`: local JSON persistence, Windows environment variable reader/writer for secret-only hybrid mode, Cloudflare D1/Secrets Store remote mode, redaction checks, and per-provider credential validation.
- `runtime-bootstrapper`: first-run dependency inventory, install/update/configuration plan, operator authorization, background execution, CLI authentication flow, and final readiness report.
- `capability-probe`: pre-session CLI availability/model probe with failure classes.
- `editorial-session`: job lifecycle, phase transitions, round creation, formal state tracking.
- `status-parser`: strict parser for structured agent verdicts.
- `convergence-engine`: strict-only convergence, persisted per-round snapshots.
- `protocol-engine`: executable gates compiled from Protocolo Editorial v1.10.0.
- `abnt-citation-engine`: automatic ABNT NBR 10520:2023 and NBR 6023 citation/reference formatting plus blocker generation.
- `maestro-peer`: deterministic fourth-peer verdict based on protocol, evidence, citation, export, and MainSite compatibility gates.
- `protocol-library`: UI and storage for importing, replacing, diffing, activating, and archiving mutable editorial protocol documents.
- `evidence-engine`: link, DOI, ISBN, catalog, PDF, and freshness verification.
- `web-evidence-engine`: fetch, curl-compatible replay, web search connectors, rendered fetch, default-browser assisted capture, robots/ToS/copyright state, and source hashing.
- `link-integrity-engine`: extraction, validation, sanitization, correction proposals, and cross-review escalation for every generated/imported link.
- `quarantine-ledger`: bibliographic quarantine records.
- `claim-map`: argument-support map by claim, source, locator, certainty, and risk.
- `posteditor-parity-editor`: MainSite-bound editor copied/adapted from `admin-app/MainSite/PostEditor`, with the same effective TipTap extension set and HTML output contract.
- `shared-chat-importer`: ChatGPT, Claude, and Gemini shared-link classification, browser-capable extraction, Markdown conversion, and provenance capture.
- `mainsite-d1-bridge`: guarded read/write/import/export bridge for `bigdata_db.mainsite_posts`, using Cloudflare API as the primary path and `wrangler@latest` only as fallback.
- `json-store`: event-sourced JSON/NDJSON persistence with locks, atomic writes, checksums, and recovery.
- `cloudflare-persistence-bridge`: API-first provisioning and synchronization for D1 `maestro_db`, schema migrations, Cloudflare Secrets Store, and secret reference mapping.
- `exporter`: Markdown, Markdown plus HTML, PDF, MainSite-compatible HTML, internal audit report, and semantic diff.

## 4.1 Integrated Editor Gate

TipTap is approved only as PostEditor parity. Maestro must not maintain a separate "similar" TipTap editor for MainSite content.

Required parity surface:

- Same practical toolbar capabilities as PostEditor.
- Same custom extensions, node views, tables, task lists, mentions, images, YouTube embeds, search/replace, slash commands, and Markdown import semantics.
- Same `editor.getHTML()` save contract plus target/rel link normalization.
- Same sanitizer and `PostReader` compatibility checks before direct D1 publishing is stable.

The current compatibility module lives in `src/editor/posteditor/`. Future changes in `admin-app/MainSite/PostEditor` require equivalent Maestro review.

## 5. JSON Store Design

SQLite is rejected for v1 because the operator requirement is JSON files.

Use an event-sourced file layout:

```text
data/
  config/
    app-config.json
    model-pins.json
    protocol-pins.json
  sessions/
    <session-id>/
      manifest.json
      events.ndjson
      rounds/
        round-001.prompt.md
        round-001.codex.json
        round-001.claude.json
        round-001.gemini.json
        round-001.convergence.json
      evidence/
        links.ndjson
        catalog.ndjson
        pdf-extracts.ndjson
      ledgers/
        bibliographic-quarantine.json
        claim-map.json
        blockers.json
        semantic-diff.json
      exports/
        texto-final.md
        ata-da-sessao.md
        mainsite-content.html
        audit-report.md
```

Writes use temp-file plus rename. Multi-process writes use a directory lock with TTL. Every immutable artifact includes `schema_version`, `created_at`, `sha256`, and `source_inputs`.

## 6. Editorial Protocol Gates

The protocol engine must encode at least these v1.10.0 gates for the initial protocol profile:

- ABNT NBR 10520:2023 and NBR 6023 citation/reference shape.
- Direct quotes require page, paragraph, item, chapter/verse, or other verifiable locator.
- Famous phrases in quotes are treated as direct quotes.
- PDF/e-reader/browser pagination is not editorial pagination unless verified.
- Compound surnames are never truncated, including the Matta e Silva rule.
- Apparatus order is mandatory: normalized ABNT references, online consultable sources, complementary readings.
- Bibliographic quarantine table with fields: `obra`, `autor`, `tipo_de_fonte`, `papel_no_texto`, `edicao_consultada`, `primeira_edicao`, `localizador`, `url_ou_catalogo`, `status_de_verificacao`, `risco_se_errada`.
- Named-author regime: structural mobilization, lateral mobilization, or generic mention.
- Final apparatus closes after the body stabilizes.
- Argument-support map with claim type, source, locator, certainty, and risk.
- Canonical existence check before edition/frontispiece check.
- Corpus whitelists for closed corpora, especially Matta e Silva's nine works.
- Confabulation taxonomy: editorial anachronism, first-edition/consulted-edition collage, presumed date, unconsulted page, associative authorship, epithets/pseudonyms, wrong imprint, phantom title.
- Anti-anachronism check for publisher/year pairs, including Brazilian CNPJ/RFB verification when needed.
- Five-level epistemic access declaration: full document opened, excerpt consulted, consolidated memory, contextual inference, unverified hypothesis.
- Wikipedia never counts as a citably valid source or verification point.
- Primary source, access repository, qualified secondary source, tertiary source, and prohibited source are distinct categories.
- At least ten substantive verification points for long texts unless an honest deficit is declared.
- Weak links never exist only to satisfy a count.
- Freshness rules for unstable facts: six months for fast-moving domains, eighteen months for moderate-moving domains, stable historical facts by traceability.
- Blog-ready YAML/Markdown checks: no `author`, no `is_published` unless explicitly required, `display_order: 1`, boolean `is_pinned`, title equals H1, strict Markdown, no HTML/shortcodes/placeholders.
- Sensitive clinical, legal, financial, regulatory, political, and journalistic claims require appropriate safeguards and sources.
- Historical shadow proportionality for problematic authors/traditions.
- Non-equivalence warning in comparative tradition tables.
- No protocol self-reference in publicable final text.
- Four blocker axes: technical, factual-epistemic, clinical-legal-financial, stylistic-formal.
- Formal states: `rascunho`, `revisao_editorial`, `auditoria_bibliografica`, `pre-publicavel`, `publicavel`.
- Semantic diff: corrected facts, removed references, added references, quote/paraphrase conversions, YAML/Markdown structural changes, remaining pending items.

## 7. Protocol Library UI

Maestro must include a dedicated screen for protocol management.

Required capabilities:

- Attach/import a Markdown protocol file at any time.
- Capture declared protocol version, source file name, import timestamp, importer app version, sha256 hash, and optional operator notes.
- Store the imported protocol under `./data/protocols/` as JSON metadata plus the original Markdown content.
- Mark one protocol version as the default for new sessions.
- Pin each session to exactly one protocol snapshot at intake. Later protocol updates do not mutate old sessions.
- Show a semantic diff between protocol versions before activation.
- Run a protocol-lint pass that extracts headings, checklists, mandatory terms, annexes, blocked-source rules, and version identifiers.
- Allow an operator-authored changelog note for each protocol update.
- Never commit operator-supplied protocols to Git by default. Public repo fixtures use synthetic sample protocols only.

Because AI models will keep finding failures and improvements in the editorial rules, protocol update is a first-class workflow, not a maintenance afterthought.

## 7.1 Background Agent UX

Claude CLI, Codex CLI, and Gemini CLI are runtime workers, not visible terminal sessions. The operator experience must keep them in background and translate their activity into clear UI states:

- Current action, phase, progress, and blocker indicators.
- Agent status cards using `READY`, `NOT_READY`, and `NEEDS_EVIDENCE`.
- A UI verbosity control with at least summary, detailed, and diagnostic levels.
- Diagnostic view may show structured event names, retry classes, and log paths, but not raw prompt dumps, secrets, stdout, stderr, or full transcripts.
- Detailed forensic material remains in ignored local JSON/NDJSON logs and can be attached manually for analysis.

The UI should feel operational and calm: enough transparency for trust, without turning the app into a terminal multiplexer.

## 8. Evidence Engine

Maestro supplies evidence packs to the agents. Agents may reason about evidence, but Maestro performs the fetch/check work itself.

Minimum v1 scope:

- HTTP HEAD/GET verification with redirect chain, status, content type, final URL, access timestamp, retry class, and hash when content is downloaded.
- PDF detection and text extraction for quote-locator checks where feasible.
- DOI checks through Crossref where feasible.
- ISBN/catalog checks through public catalog sources where feasible.
- Internet Archive, Project Gutenberg, HathiTrust, Google Books, institutional repositories, and official portals as access repositories, not primary-source substitutes.
- Receita Federal/CNPJ or documented secondary mirrors for publisher anachronism checks, with official source precedence.
- Link weakness classifier: homepage facade, generic portal, transient URL, non-specific commercial page, paywall, broken link, forbidden access, content-type mismatch.
- Freshness classifier by domain.
- Evidence cache as JSON with TTL by source class and explicit revalidation records.

The web evidence engine should behave like a careful human researcher using a browser. It may open URLs in a Maestro-assisted browser window or in the operator's default browser for explicit operator capture, and it may use WebView2 with Maestro's own app-local user data folder. It must not silently copy the user's active browser cookies, session profile, password stores, or fingerprint data.

Interactive states such as `captcha_required`, `login_required`, `consent_required`, `download_confirmation`, `paywall`, `auth_required`, and `forbidden` are first-class evidence results. They pause automation, open the assisted browser/default browser flow, let the operator act, then resume with `human_resolved` provenance when appropriate.

## 9. Agent Workflow

Phases:

1. Intake brief.
2. Protocol profile and risk map.
3. Outline.
4. Draft.
5. Mechanical lint and evidence collection.
6. Bibliographic quarantine.
7. Multi-agent editorial audit.
8. Revision.
9. Semantic diff.
10. Final convergence.
11. Export.

Final convergence is accepted only when:

```text
Claude READY
Codex READY
Gemini READY
MaestroPeer READY
same accepted final round
```

`MaestroPeer` is computed by the app, not by a model. It must mark `NOT_READY` or `NEEDS_EVIDENCE` when ABNT formatting, source verification, protocol compliance, link evidence, or export structure is unresolved.

Link failures are not cosmetic. Broken, invented, weak, redirected-to-wrong-content, or unsupported links must create blockers and feed the next cross-review round with the mechanical evidence and correction candidates.

Each AI response must close with a structured status block:

```json
{
  "status": "READY | NOT_READY | NEEDS_EVIDENCE",
  "confidence": "verified | inferred | unknown",
  "evidence_sources": [],
  "blockers": [],
  "caller_requests": [],
  "follow_ups": []
}
```

`confidence: unknown` must pair with `NEEDS_EVIDENCE`.

## 10. Failure Handling

- First-run bootstrap failures block full operation until resolved, skipped, or explicitly deferred with a degraded-mode warning.
- Pre-session probe detects missing CLI, auth failure, model rejection, timeout, rate limit, and prompt-safety rejection.
- If all peers are unavailable, session aborts.
- If one peer is unavailable, Maestro may run a degraded audit, but the result cannot be called full trilateral convergence.
- Mid-round transient failures retry once with backoff and record attempts.
- Max rounds default: 8. Hitting the cap finalizes as `max-rounds`, not `publicavel`.
- `max-rounds` is a pause/escalation state, not a final-delivery state. It may produce diagnostics or a non-publicable working draft, but it cannot produce `texto-final.md`.
- Rate limit is a distinct failure class, not a content objection.
- `2 READY + 1 NOT_READY/NEEDS_EVIDENCE/status_missing` is non-convergence and creates the next round until resolved or `max-rounds` is reached.
- Protocol violations are distinct failure classes: malformed status block, peer impersonation, model downgrade evidence, unilateral writes during design-only review, or missing required evidence fields.

## 11. Language Policy

- User-facing UI, editorial drafts, internal editorial reports, and exported Markdown: Portuguese of Brazil by default.
- Machine JSON keys: stable ASCII snake_case.
- Agent prompts may include pt-BR editorial content, but orchestration instructions and status contracts should remain compact and schema-driven to reduce parser drift.

## 12. Testing Baseline

- Mock CLI adapters for Codex, Claude, and Gemini.
- Golden fixtures for status parsing, convergence, blocker taxonomy, JSON recovery, quarantine records, claim maps, link classifications, and Markdown/YAML lint.
- No real CLI calls in unit tests.
- Real CLI smoke tests are opt-in, slow, and clearly separated.
- Evidence engine has network fixtures plus live opt-in probes.
- Test fixtures must not contain secret-shaped dummy values. Use obviously synthetic placeholders such as `<api_key_redacted>` rather than strings that resemble real provider keys.

## 13. Repository Hygiene

The repository is assumed private during early synchronization and public later. The same ignore policy applies from the first commit:

- Commit source, documentation, schemas, deterministic fixtures, sample configs, and generated legal/open-source metadata.
- Do not commit `data/`, runtime sessions, evidence caches, exports, logs, browser user-data folders, local vaults, `.env*`, CLI transcripts, prompt dumps, OneDrive copies, copied private editorial protocols, or real editorial drafts unless the operator explicitly designates them as public fixtures.
- Keep `.env.example` and `config.example.json` sanitized and documented.
- CI must include secret scanning before public release.
- Add Dependabot configuration for npm, GitHub Actions, and Rust/Cargo once those manifests exist.
- Use GitHub CodeQL Default Setup for code scanning. Treat alerts as release blockers unless explicitly triaged.
- README, LICENSE, SECURITY, CONTRIBUTING, CODE_OF_CONDUCT, CHANGELOG, release notes, package metadata, and GitHub Packages publication plan are day-zero deliverables, not post-maturity cleanup.
- GitHub Releases use annotated tags and generated release notes only after local validation and pre-release audit.
- GitHub Packages publishing is planned but disabled until package identity, privacy posture, and token handling are settled.
- GitHub Sponsors support is active through `.github/FUNDING.yml`, mirroring the public `cross-review-mcp` funding model with the Maestro Pages URL.
- GitHub Pages uses the modern GitHub Actions artifact deployment model from `site/`, not a legacy `gh-pages` publishing branch.
- Dependabot starts with GitHub Actions updates and enables npm/Cargo blocks as soon as the corresponding manifests exist.

## 14. Open Implementation Checks

- Verify the exact Tauri API path for setting an absolute WebView2 data directory under `./data/webview`.
- Verify portable distribution layout and whether a fixed WebView2 runtime should be bundled or only detected.
- Choose the first set of external catalog APIs after validating availability, terms, and rate limits.
- Decide whether the optional encrypted JSON vault is in v1 or deferred.
- Decide the first public package names for GitHub Packages and whether app binaries are released only through GitHub Releases or also package registries.
- Pin the verified stable Rust toolchain with `rust-toolchain.toml` before Tauri scaffolding.
