# CLI Agent Audit

Status: implementation gate.
Date: 2026-04-26.

This audit records what Maestro must rely on, verify, and defend against when orchestrating Codex CLI, Claude CLI, and Gemini CLI in background.

It is not enough for a CLI to "answer a prompt". Maestro needs predictable non-interactive execution, auth probing, structured output, exit-code handling, stderr capture, tool/permission controls, model provenance, and safe update behavior.

## Local Snapshot

Observed on this Windows 11+ development machine:

| Agent | Local command | Local version | Headless smoke | Structured output |
| --- | --- | --- | --- | --- |
| Codex | `codex` | `0.124.0` | `codex exec --ephemeral --json "Return exactly: READY"` returned `READY` | JSONL events |
| Claude | `claude` | `2.1.119` | `claude -p "Return exactly: READY" --output-format json --no-session-persistence` returned `READY` | JSON object |
| Gemini | `gemini` | `0.39.1` | `gemini -p "Return exactly: READY" --output-format json` returned `READY` | JSON object plus terminal warning noise |

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
