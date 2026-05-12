# Editorial Session Workflow

Status: implementation contract with functional background, hardened resume pass in `v0.3.5`, startup crash recovery hardening in `v0.3.6`, long-running agent diagnostics in `v0.3.7`, DeepSeek API peer support in `v0.3.11`, per-session controls/log readability in `v0.3.13`, Grok/xAI as fifth API peer in `v0.5.16`, provider prompt-cache policy telemetry in `v0.5.19`, and Perplexity/Sonar as sixth API-only peer in `v0.5.27`.

Maestro's core workflow starts from an operator prompt and an active editorial protocol. The app must not deliver a final text until every AI peer selected for that session and Maestro's deterministic local checks all return `READY` in the same accepted round.

This is inviolable: the final text is delivered only after unanimous acceptance from the selected peer set. While there is any divergence, the work remains open. Optional time and cost limits do not relax consensus; when reached, they pause/stop the session without `texto-final.md`.

## Intake

The operator provides:

- Session title.
- Generation prompt.
- Active editorial protocol snapshot.
- Optional source files/anexos, public HTTP/HTTPS links, shared chat links, PDFs, Markdown, HTML, and MainSite post references.
- Active AI peer set: 1 to 6 among Claude, Codex, Gemini, DeepSeek, Grok, and Perplexity.
- Optional max session time in minutes. Blank means ignored.
- Optional max observed direct-API cost in USD. Blank means ignored.

Each session pins the protocol by file name, declared version when available, import timestamp, byte size, line count, and SHA-256 hash.

## Mandatory Protocol Reading

Before any drafting or review round, Maestro must force each active agent to read the entire protocol line by line.

Minimum proof record per agent:

- `protocol_hash`
- `line_count_expected`
- `line_count_acknowledged`
- `read_mode: full_line_by_line`
- `acknowledged_sections`
- `missing_ranges`
- `status`

If any agent cannot confirm full reading, the round cannot start.

If any peer remains `NOT_READY` or `NEEDS_EVIDENCE`, the session continues, pauses for operator evidence, or exports only a non-publicable working draft. It must not emit `texto-final.md`.

## Round Flow

`v0.3.1` introduced the real executable pass:

- The UI blocks execution unless the full Markdown protocol text was imported.
- Claude, Codex, and Gemini are called without visible terminal windows in Windows release builds when CLI transport is selected. DeepSeek, Grok, and Perplexity are API-only peers and therefore participate only in API or hybrid transport.
- Real editorial calls run without artificial timeout; model latency is treated as normal operational time.
- The UI remains responsive and shows heartbeat progress while the native worker waits for long-running agents.
- Draft generation can fall back across the selected active agents when an earlier agent fails to produce usable text.
- Selected active agents are called in background to review the draft, excluding the current draft/revision author.
- From `v0.3.13`, only the selected peers participate in draft fallback, revision fallback, and review consensus. Unselected peers do not run and cannot block final delivery.
- If a review returns `NOT_READY`, Maestro feeds the review artifacts into a revision prompt and starts another round instead of treating divergence as task completion.
- If an operational failure prevents progress, Maestro writes the session minutes and pauses without emitting a final text.
- Each agent receives the full protocol text and operator prompt.
- Each review must return `MAESTRO_STATUS: READY` as its first line to count as approval.
- Maestro writes `texto-final.md` only when all editorial review peers return `READY` and the draft command succeeded.
- Maestro always writes `ata-da-sessao.md` plus per-agent artifacts under ignored `data/sessions/<run>/`.
- Maestro writes `session-contract.json`, `cost-ledger.json` when direct API costs are observed, `cache-manifest.ndjson` when API peers configure provider prompt cache policy, `links.json`/`links.md` when links are supplied, and `attachments/manifest.json` when attachments are supplied.

`v0.3.4` added resumable interrupted sessions, with filesystem scan hardening in `v0.3.5`:

- Maestro scans `data/sessions/` for sessions that have `prompt.md` and `protocolo.md` but no `texto-final.md`.
- If one interrupted session exists, the UI resumes it directly; if several exist, the operator chooses which one to continue.
- Maestro recovers the latest usable draft or revision artifact and continues review from that round instead of restarting from zero.
- If the operator has imported a new protocol before resuming, that protocol is passed to the selected active agents; the previous `protocolo.md` is preserved as a local `protocolo-anterior-*.md` artifact before the new protocol becomes active.
- If no new protocol is loaded, Maestro uses the `protocolo.md` saved inside the session folder.

`v0.3.7` adds an explicit trace for the "apparently stopped" state:

- Before each CLI child process is launched, Maestro writes the target Markdown artifact with `Status: RUNNING` and empty stdout/stderr blocks.
- After spawn, Maestro records the child PID, resolved executable path, role, round artifact path, and output counters.
- While a child process remains active, Maestro logs `session.agent.running` every 30 seconds with elapsed seconds and stdout/stderr bytes captured so far.
- If the app is closed or crashes mid-agent, the unfinished `RUNNING` artifact remains as evidence but is ignored as a usable draft during resume.
- Spawn or execution errors are now written as parseable Markdown artifacts instead of bare one-line error text.
- If stdin delivery fails after a child process has been spawned, Maestro kills and reaps that child before recording the parseable execution error artifact.

`v0.3.11` expands draft-lead selection with DeepSeek:

- The operator can choose Claude, Codex, Gemini, or DeepSeek as the draft lead before starting a session.
- The selected draft lead is saved in `prompt.md`, writes the first version for new sessions, and is tried first in revision rounds; the other agents remain available as fallback if that agent path cannot produce usable text.
- Resumed sessions keep the saved draft lead when present, so changing the visible picker later does not silently reorder an existing session.
- Direct API peers record `openai-api`, `anthropic-api`, `gemini-api`, or `deepseek-api` artifacts in the same `agent-runs` directory as CLI agents.
- Editorial CLI processes run with their current directory set to the session `agent-runs` folder, not the portable app root.
- Any working draft remains an internal session artifact. Root-level `draft.md` is not a supported output and must not be treated as `texto-final.md`.

This pass is still conservative. The deterministic link checker, ABNT engine, cancellable sessions, and MainSite D1 publication gates remain required before the workflow can be considered mature.

`v0.3.13` adds bounded per-session controls:

- Peer selection is explicit and persisted. Resumes use the saved session contract unless the operator starts a new run or explicitly overrides controls.
- The time limit is a hard session deadline. If reached before or during a call, Maestro records `TIME_LIMIT_REACHED` and keeps the session private/paused.
- The cost limit is a hard session stop, not a peer drop. Consensus is never redefined by silently removing a selected peer.
- Direct observed API cost is checked against the single session-level USD budget. The budget is not per model, and consensus is never redefined by silently removing a selected peer.
- Provider tariffs are mandatory UI configuration in `Configuracoes > Agentes via API > Tabela de tarifas`; there is no env-var fallback. Any selected peer that will run via direct provider API is blocked before invocation if its provider tariff is blank. CLI peers remain labeled as subscription/unknown cost and do not decrement the USD budget.
- Attachments are capped at 8 files, 25 MiB per file, and 75 MiB total. Small text-like files get bounded previews. CLI peers receive local paths/manifest; direct API peers receive native file/media parts when their provider supports the attachment type and the file is within the native inline size cap, and unsupported/oversized-native types remain manifest/path only.
- The session UI shows a pre-run delivery hint per attachment and per active API provider, so mixed support is explicit: a file can be native for Gemini while remaining manifest/previews for OpenAI, Anthropic, DeepSeek, Grok, or Perplexity.
- Direct API cost projection includes the native attachment payloads before the paid call is made, so a large supported file cannot bypass the optional USD session cap.
- Links must be public-looking HTTP/HTTPS URLs. Localhost, loopback/private IPs, `file:`, `data:`, and similar schemes are rejected.

`v0.5.19` adds provider prompt-cache policy:

- Prompt cache is a cost optimization only. It must not downgrade models, disable thinking, shorten the editorial protocol, or change consensus semantics.
- OpenAI/Codex and Grok/xAI direct API calls send deterministic `prompt_cache_key` values. OpenAI models that support extended retention also receive `prompt_cache_retention: "24h"`.
- Anthropic/Claude direct API calls mark the stable `system` text block with `cache_control: { "type": "ephemeral" }` and record provider cache read/create token usage when returned.
- DeepSeek uses its provider-side automatic prefix/disk cache and records hit/miss token usage when returned.
- Gemini keeps the normal thinking-preserving GenerateContent flow. Maestro records Gemini cached-token usage when returned by `usageMetadata`.
- Perplexity/Sonar currently has no documented prompt-cache control comparable to the other direct editorial flows. Maestro does not add invented cache fields and records only non-secret source/cache-plan metadata when available.
- Logs and artifacts store only cache mode, key hash, retention label, and token counts. They never store raw API keys, full prompts, protocols, or cache keys that could reveal private content.

1. Maestro builds an evidence pack and protocol pack.
2. Each agent receives the same session prompt, protocol snapshot, evidence pack, and status schema.
3. Maestro records every agent response as structured JSON plus a human-readable Markdown excerpt.
4. Maestro parses `READY`, `NOT_READY`, and `NEEDS_EVIDENCE`.
5. `NEEDS_EVIDENCE` triggers mechanical verification before the next round.
6. `NOT_READY` triggers revision or targeted debate.
7. A final text is accepted only when all selected AI peers and MaestroPeer return `READY` in the same round.
8. MaestroPeer is computed by Maestro from protocol, evidence, ABNT citation, export, and MainSite compatibility gates, and may block publication independently of the selected AI agents.
9. Link failures, hallucinated URLs, or weak links are sent into cross-review with fetch/render evidence and correction candidates.

## Discussion Screen

The UI must show a calm operational view, not raw terminal output:

- Active phase and progress.
- Protocol reading gate.
- Per-agent status.
- Round timeline.
- Evidence requests.
- Current blockers.
- Export readiness.

Diagnostic verbosity may show event names, file paths, retry classes, and status block summaries. Raw prompts, stdout, stderr, and credentials stay out of the normal UI.

Raw NDJSON remains the canonical machine-readable diagnostic format. From `v0.3.13`, Maestro also writes a human-readable projection under `data/logs/human/` with one-line summaries, selected fields, and heartbeat collapse so operators do not need to read giant JSON lines for routine diagnosis.

## Final Artifacts

Every successful session exports two separate Markdown files:

```text
texto-final.md
ata-da-sessao.md
```

`texto-final.md` contains only the approved final text.

`texto-final.md` is created only after every selected AI peer and MaestroPeer are unanimously `READY`.

`ata-da-sessao.md` contains the session record:

- Session manifest.
- Operator prompt.
- Protocol identity and hash.
- Protocol reading confirmations.
- Round timeline.
- Agent positions.
- Evidence requests and resolutions.
- Link integrity ledger and accepted corrections.
- Bibliographic quarantine decisions.
- Semantic diff.
- Final unanimity declaration.
- MaestroPeer citation/evidence/protocol verdict.

The session minutes are private operational material by default. They are useful for fine adjustment and for understanding how the agents reached the final text, but they must not be published accidentally.
