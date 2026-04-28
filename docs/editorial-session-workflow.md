# Editorial Session Workflow

Status: implementation contract with functional background, hardened resume pass in `v0.3.5`, startup crash recovery hardening in `v0.3.6`, and long-running agent diagnostics in `v0.3.7`.

Maestro's core workflow starts from an operator prompt and an active editorial protocol. The app must not deliver a final text until Claude, Codex, and Gemini all return `READY` in the same trilateral round and Maestro's deterministic fourth-peer check also returns `READY`.

This is inviolable: no matter the time, cost, number of rounds, or operational inconvenience, the final text is delivered only after unanimous acceptance. While there is any divergence, the work remains open.

## Intake

The operator provides:

- Session title.
- Generation prompt.
- Active editorial protocol snapshot.
- Optional source files, shared chat links, PDFs, Markdown, HTML, and MainSite post references.

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
- Claude, Codex, and Gemini are called without visible terminal windows in Windows release builds.
- Real editorial calls run without artificial timeout; model latency is treated as normal operational time.
- The UI remains responsive and shows heartbeat progress while the native worker waits for long-running agents.
- Draft generation can fall back across Claude, Codex, and Gemini when an earlier agent fails to produce usable text.
- Claude, Codex, and Gemini are called in background to review the draft.
- If a review returns `NOT_READY`, Maestro feeds the review artifacts into a revision prompt and starts another round instead of treating divergence as task completion.
- If an operational failure prevents progress, Maestro writes the session minutes and pauses without emitting a final text.
- Each agent receives the full protocol text and operator prompt.
- Each review must return `MAESTRO_STATUS: READY` as its first line to count as approval.
- Maestro writes `texto-final.md` only when all three review peers return `READY` and the draft command succeeded.
- Maestro always writes `ata-da-sessao.md` plus per-agent artifacts under ignored `data/sessions/<run>/`.

`v0.3.4` added resumable interrupted sessions, with filesystem scan hardening in `v0.3.5`:

- Maestro scans `data/sessions/` for sessions that have `prompt.md` and `protocolo.md` but no `texto-final.md`.
- If one interrupted session exists, the UI resumes it directly; if several exist, the operator chooses which one to continue.
- Maestro recovers the latest usable draft or revision artifact and continues review from that round instead of restarting from zero.
- If the operator has imported a new protocol before resuming, that protocol is passed to Claude, Codex, and Gemini; the previous `protocolo.md` is preserved as a local `protocolo-anterior-*.md` artifact before the new protocol becomes active.
- If no new protocol is loaded, Maestro uses the `protocolo.md` saved inside the session folder.

`v0.3.7` adds an explicit trace for the "apparently stopped" state:

- Before each CLI child process is launched, Maestro writes the target Markdown artifact with `Status: RUNNING` and empty stdout/stderr blocks.
- After spawn, Maestro records the child PID, resolved executable path, role, round artifact path, and output counters.
- While a child process remains active, Maestro logs `session.agent.running` every 30 seconds with elapsed seconds and stdout/stderr bytes captured so far.
- If the app is closed or crashes mid-agent, the unfinished `RUNNING` artifact remains as evidence but is ignored as a usable draft during resume.
- Spawn or execution errors are now written as parseable Markdown artifacts instead of bare one-line error text.
- If stdin delivery fails after a child process has been spawned, Maestro kills and reaps that child before recording the parseable execution error artifact.

`Unreleased` adds draft-lead selection and workspace isolation:

- The operator can choose Claude, Codex, or Gemini as the draft lead before starting a session.
- The selected draft lead is saved in `prompt.md`, writes the first version for new sessions, and is tried first in revision rounds; the other agents remain available as fallback if that CLI cannot produce usable text.
- Resumed sessions keep the saved draft lead when present, so changing the visible picker later does not silently reorder an existing session.
- Editorial CLI processes run with their current directory set to the session `agent-runs` folder, not the portable app root.
- Any working draft remains an internal session artifact. Root-level `draft.md` is not a supported output and must not be treated as `texto-final.md`.

This pass is still conservative. The deterministic link checker, ABNT engine, cancellable sessions, and MainSite D1 publication gates remain required before the workflow can be considered mature.

1. Maestro builds an evidence pack and protocol pack.
2. Each agent receives the same session prompt, protocol snapshot, evidence pack, and status schema.
3. Maestro records every agent response as structured JSON plus a human-readable Markdown excerpt.
4. Maestro parses `READY`, `NOT_READY`, and `NEEDS_EVIDENCE`.
5. `NEEDS_EVIDENCE` triggers mechanical verification before the next round.
6. `NOT_READY` triggers revision or targeted debate.
7. A final text is accepted only when Claude, Codex, Gemini, and MaestroPeer all return `READY` in the same round.
8. MaestroPeer is computed by Maestro from protocol, evidence, ABNT citation, export, and MainSite compatibility gates, and may block publication independently of the three AI agents.
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

## Final Artifacts

Every successful session exports two separate Markdown files:

```text
texto-final.md
ata-da-sessao.md
```

`texto-final.md` contains only the approved final text.

`texto-final.md` is created only after Claude, Codex, Gemini, and MaestroPeer are unanimously `READY`.

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
