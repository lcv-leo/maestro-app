# Diagnostic Logging

Maestro records structured diagnostic events as newline-delimited JSON under:

```text
data/logs/maestro-<timestamp>-pid<id>.ndjson
```

Each app execution creates a new NDJSON file. Events inside one app execution are appended only to that execution's file and include a `session.id` plus `session.log_file`.

During early development the logs are intentionally detailed. They should make UI actions, protocol imports, native runtime startup, frontend errors, unhandled promise rejections, session context, agent context, evidence context, and file paths understandable without replaying the whole session.

CLI agents run silently in background. On Windows release builds, child processes are created without visible terminal windows. The UI shows synthesized status, elapsed-time heartbeat progress, and blockers; it does not expose raw terminal output as the normal operator experience. Detailed CLI lifecycle events, sanitized command metadata, parsed statuses, exit codes, durations, timeout policy, and artifact paths belong in structured logs so they can be analyzed without forcing the operator to read terminal transcripts.

The operator-facing interface must stay human-readable. Long diagnostic histories are summarized as latest-round status, elapsed time, and clear next state; repeated rows scroll inside bounded panels instead of stretching the full window. Technical names such as `session.agent.finished`, exit codes, stdout/stderr byte counters, and raw artifact paths remain in NDJSON and session files for support analysis.

Raw prompts, full protocol text, stdout, and stderr are written to ignored session artifacts under `data/sessions/<run>/`. The NDJSON points to those artifacts and keeps only safe summaries.

When a prompt bundle is too large for reliable stdin delivery, Maestro writes an ignored sidecar file beside the agent artifact, such as `round-072-codex-review-input.md`, and sends the CLI a compact instruction to read that local file. Logs record both compact stdin size and original prompt size so pipe failures can be diagnosed without exposing the full prompt in NDJSON.

First-run bootstrap events use the same logging policy. Dependency scans, install plans, background installer exit codes, authentication handoffs, and post-install probes must be captured as structured events with secrets redacted.

## What To Send For Diagnosis

When asking Codex to diagnose a Maestro issue, attach the latest `data/logs/*.ndjson` file and describe the visible symptom.

Each log line is standalone JSON with:

- `timestamp`
- `native_log_sequence`
- `level`
- `category`
- `message`
- `context`
- `app`
- `process`
- `session`

Frontend events include a `context.runtime` snapshot with viewport, current URL/hash, visibility, online state, active element, user agent, screen metrics, device pixel ratio, language, platform, hardware concurrency, and browser connection hints when available.

Native startup events include resolved command paths for known CLIs when available. Editorial agent events include `session.agent.started` and `session.agent.finished` records with run id, agent, role, CLI, duration, exit code, timeout policy, output path, stdout/stderr character counts, API token usage, cost, and provider cache telemetry when an API peer reports it. Native command execution drains stdout and stderr while the process is running, so large agent outputs do not block only because an OS pipe buffer filled.

Prompt-cache policy events use `session.provider.cache.configured`. They record only provider, model, role, cache mode, cache-key hash, retention label, stable-prefix character count, and prompt character count. The companion `data/sessions/<run>/cache-manifest.ndjson` file repeats this non-secret metadata per API call so long sessions can be audited without scanning the full raw log. Cache hit/miss/read/create token counters appear in `session.agent.finished` when returned by the provider.

Real editorial agent calls do not have an artificial timeout. Timeout is allowed only for short diagnostics and dependency probes. While a real editorial call is still running, the frontend emits `session.editorial.heartbeat` events so a log reader can distinguish normal long model latency from an app freeze.

Recommended bootstrap categories:

- `bootstrap.scan.started`
- `bootstrap.dependency.status`
- `bootstrap.install.plan`
- `bootstrap.install.started`
- `bootstrap.install.completed`
- `bootstrap.auth.required`
- `bootstrap.auth.completed`

Recommended credential categories:

- `settings.credential_storage.changed`
- `settings.cloudflare.verify_requested`
- `settings.cloudflare.verify_completed`
- `settings.ai_provider.verify_requested`
- `settings.ai_provider.verify_completed`
- `settings.windows_env.write_requested`
- `settings.windows_env.write_completed`

Recommended editorial categories:

- `session.prompt.submitted`
- `session.protocol.pinned`
- `session.editorial.requested`
- `session.editorial.started`
- `session.editorial.heartbeat`
- `session.agent.started`
- `session.provider.cache.configured`
- `session.agent.finished`
- `session.editorial.completed`
- `session.editorial.blocked`
- `session.review.not_ready`
- `session.revision.unavailable`
- `session.editorial.failed`
- `native.panic`

## Redaction

The logger redacts common secret-shaped strings even when embedded in URLs, JSON fragments, or header-like text, plus sensitive keys such as raw `token`, `secret`, `password`, `credential`, `authorization`, `cookie`, private keys, and API-key values. Credential logs may record safe metadata such as presence flags, env-var names, provider names, source/scope, validation status, and token kind, but never raw values. Even so, logs are operational artifacts and remain ignored by Git.

## Policy

- Logs stay inside the app folder.
- Logs are never committed.
- Runtime paths are covered by `.gitignore`.
- The logging detail level may be reduced only after the app reaches a stable enough release.
- UI verbosity changes what the operator sees, not what the diagnostic logger records during the unstable phase.
