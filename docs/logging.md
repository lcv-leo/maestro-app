# Diagnostic Logging

Maestro records structured diagnostic events as newline-delimited JSON under:

```text
data/logs/maestro-<timestamp>-pid<id>.ndjson
```

Each app execution creates a new NDJSON file. Events inside one app execution are appended only to that execution's file and include a `session.id` plus `session.log_file`.

During early development the logs are intentionally detailed. They should make UI actions, protocol imports, native runtime startup, frontend errors, unhandled promise rejections, session context, agent context, evidence context, and file paths understandable without replaying the whole session.

CLI agents run silently in background. The UI shows synthesized status, progress, and blockers; it does not expose raw terminal output as the normal operator experience. Detailed CLI lifecycle events, sanitized command metadata, parsed statuses, exit codes, durations, timeout flags, and artifact paths belong in structured logs so they can be analyzed without forcing the operator to read terminal transcripts.

Raw prompts, full protocol text, stdout, and stderr are written to ignored session artifacts under `data/sessions/<run>/`. The NDJSON points to those artifacts and keeps only safe summaries.

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

Native startup events include resolved command paths for known CLIs when available. Editorial agent events include `session.agent.started` and `session.agent.finished` records with run id, agent, role, CLI, duration, exit code, timeout, output path, and stdout/stderr character counts. Native command execution drains stdout and stderr while the process is running, so large agent outputs do not block only because an OS pipe buffer filled.

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
- `session.agent.started`
- `session.agent.finished`
- `session.editorial.completed`
- `session.editorial.blocked`
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
