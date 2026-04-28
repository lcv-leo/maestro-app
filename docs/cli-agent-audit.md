# CLI Agent Audit

Status: implementation gate.
Date: 2026-04-26.

This audit records what Maestro must rely on, verify, and defend against when orchestrating Codex CLI, Claude CLI, and Gemini CLI in background.

It is not enough for a CLI to "answer a prompt". Maestro needs predictable non-interactive execution, auth probing, structured output, exit-code handling, stderr capture, tool/permission controls, model provenance, and safe update behavior.

## Local Snapshot

Observed on this Windows 11+ development machine:

| Agent | Local command | Local version | Headless smoke | Structured output |
| --- | --- | --- | --- | --- |
| Codex | `codex` | `0.125.0` | `codex --ask-for-approval never exec --skip-git-repo-check --sandbox read-only --color never "<short prompt>"` accepted stdin appended as a `<stdin>` block and returned the requested marker; stdin-only `-` mode was observed hanging in one local probe | Text/JSONL-capable events depending on flags |
| Claude | `claude` | `2.1.119` | `claude --print --input-format text --output-format text --permission-mode dontAsk` accepted stdin and returned the requested marker | Text, JSON object, or stream JSON depending on flags |
| Gemini | `gemini` | `0.39.1` | `gemini --prompt "Read stdin and comply." --output-format text --approval-mode yolo --skip-trust` accepted stdin and returned the requested marker, with terminal warning noise possible | Text or JSON object depending on flags |

Auth was present for the local smoke tests, but Maestro must never assume the operator's machine is already authenticated.

## Official Documentation Consulted

- Codex CLI overview and upgrade: https://developers.openai.com/codex/cli
- Codex non-interactive mode: https://developers.openai.com/codex/noninteractive
- Codex authentication: https://developers.openai.com/codex/auth
- Codex approvals and security: https://developers.openai.com/codex/agent-approvals-security
- Claude Code CLI reference: https://code.claude.com/docs/en/cli-reference
- Claude Code environment variables: https://code.claude.com/docs/en/env-vars
- Claude Code permission modes: https://code.claude.com/docs/en/permission-modes
- Gemini CLI docs: https://google-gemini.github.io/gemini-cli/docs/
- Gemini CLI headless mode: https://google-gemini.github.io/gemini-cli/docs/cli/headless.html
- Gemini CLI authentication: https://google-gemini.github.io/gemini-cli/docs/get-started/authentication.html
- Gemini CLI shell tool: https://google-gemini.github.io/gemini-cli/docs/tools/shell.html
- Gemini CLI web fetch/search tools: https://google-gemini.github.io/gemini-cli/docs/tools/web-fetch.html and https://google-gemini.github.io/gemini-cli/docs/tools/web-search.html
- Gemini CLI release cadence: https://google-gemini.github.io/gemini-cli/docs/releases.html

## Suitability Findings

### Codex CLI

Codex is suitable for Maestro orchestration through `codex exec`.

Required adapter choices:

- Use `codex exec` for background work, not the interactive TUI.
- Prefer `--json` so Maestro can parse JSONL events.
- Use `--output-schema` when Maestro needs a strict final status block.
- Use `--ephemeral` unless Maestro intentionally wants Codex session files.
- Set explicit sandbox and approval flags per operation.
- Capture stdout and stderr separately. Stderr can contain plugin/cache/sync warnings and provider HTML challenge content even when the actual agent result succeeds.
- Treat Git repository requirements as a preflight condition or use `--skip-git-repo-check` only in controlled Maestro runtime folders.
- Prefer API-key auth for automation when the operator selects API mode; subscription auth remains a separate CLI transport.

### Claude CLI

Claude is suitable for Maestro orchestration through `claude -p`.

Required adapter choices:

- Use `-p` / `--print` for non-interactive runs.
- Prefer `--output-format json` or `--output-format stream-json`.
- Use `--json-schema` for strict output validation where possible.
- Use `--no-session-persistence` for stateless peer rounds unless explicit resume is needed.
- Use `--bare` for controlled scripted calls when Maestro wants to avoid auto-discovery of hooks, memories, keychain reads, plugin sync, and other ambient context.
- Set `--permission-mode` and tool allow/deny lists deliberately.
- Parse API cost and model usage from JSON output for diagnostics and operator budgeting.
- Treat `ANTHROPIC_API_KEY` as overriding subscription auth in non-interactive mode when present.

### Gemini CLI

Gemini is suitable for Maestro orchestration through headless mode, with stricter parsing guards.

Required adapter choices:

- Use `gemini -p` / `--prompt` for headless runs.
- Prefer `--output-format json` and parse a JSON object defensively.
- Expect terminal warning noise even on successful runs; do not require byte-perfect stdout.
- Use environment-variable auth for headless systems when no cached login exists.
- Track token/tool statistics from JSON output.
- Configure approval mode explicitly: `default`, `auto_edit`, `yolo`, or `plan`.
- Treat Gemini web fetch/search as model-mediated evidence, not raw mechanical verification. Maestro's own Web Evidence Engine remains authoritative for link validation.
- Monitor release cadence. Gemini stable releases are frequent; Maestro should run a version probe before long sessions.

## Cross-Agent Adapter Contract

`v0.3.1` has the current Tauri-native process adapter pass for real background sessions. It writes full per-agent artifacts under `data/sessions/<run>/agent-runs/` and logs sanitized lifecycle summaries as `session.agent.started` / `session.agent.finished`. On Windows release builds, child processes must be created without visible terminal windows. Real editorial calls have no artificial timeout; only diagnostics may use short bounded probes. The adapter is still a defensive text parser, not the final structured-output implementation described below.

Every CLI adapter must produce this internal record before a peer response can enter convergence:

```json
{
  "agent": "codex | claude | gemini",
  "transport": "cli",
  "cli_path": "absolute path",
  "cli_version": "observed version",
  "auth_status": "ready | auth_required | unknown | failed",
  "model_pin": "requested model id or alias",
  "command": "redacted command vector",
  "stdin_sha256": "hash of prompt bundle",
  "stdout_path": "ignored local artifact path",
  "stderr_path": "ignored local artifact path",
  "exit_code": 0,
  "parsed_status": "READY | NOT_READY | NEEDS_EVIDENCE | status_missing",
  "parse_warnings": [],
  "usage": {},
  "cost": {},
  "duration_ms": 0
}
```

Large prompt handling:

- Inline stdin is allowed only while the prompt bundle stays below Maestro's safe inline threshold.
- For oversized review or revision bundles, Maestro writes the full bundle to an ignored `*-input.md` sidecar file inside `data/sessions/<run>/agent-runs/` and sends the CLI a compact instruction to read that file completely before answering.
- The sidecar file is a runtime artifact, never a Git artifact.
- Agent artifacts record compact stdin size, original prompt size, and the sidecar path so support logs remain understandable without copying a very large prompt into the UI.
- If a CLI exits without output because of a transient pipe, auth, or provider failure, Maestro logs the operational failure and retries through the next review/revision cycle; it must not publish a final text without unanimous READY.

Raw stdout, stderr, prompts, and transcripts are ignored runtime artifacts. The UI receives only sanitized summaries.

## Hard Gates Before Real Adapter Implementation

- Re-run official documentation checks before coding each adapter, because all three CLIs evolve quickly.
- Add golden parser fixtures for successful JSON, JSONL streams, stderr warnings, terminal warning suffixes, malformed JSON, missing status blocks, rate limits, auth failures, and interrupted runs.
- Implement per-agent auth probes instead of inferring auth from command existence.
- Implement per-agent update probes and operator-approved update flows.
- Keep model identity explicit. If a CLI reports only an alias, Maestro must record the alias and mark the exact model as inferred unless independently attested.
- Never let CLI web fetch/search evidence substitute for Maestro's mechanical link checker.

## Open Risks

- CLI output formats are not perfectly symmetrical.
- Plugin sync, telemetry, or terminal warnings may appear outside the final answer stream.
- Auth state can depend on cached user login, API keys, environment variables, or provider-specific files.
- CLI updates can change flags, output fields, and default models.
- Cost/quota behavior differs sharply between subscription auth and API-key auth.

The adapters are therefore feasible, but they must be defensive process adapters rather than thin shell wrappers.
