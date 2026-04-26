# Diagnostic Logging

Maestro records structured diagnostic events as newline-delimited JSON under:

```text
data/logs/maestro-YYYY-MM-DD.ndjson
```

During early development the logs are intentionally detailed. They should make UI actions, protocol imports, native runtime startup, frontend errors, unhandled promise rejections, session context, agent context, evidence context, and file paths understandable without replaying the whole session.

CLI agents run silently in background. The UI shows synthesized status, progress, and blockers; it does not expose raw terminal output as the normal operator experience. Detailed CLI lifecycle events, sanitized command metadata, parsed statuses, retry classes, and evidence requests belong in structured logs so they can be analyzed without forcing the operator to read terminal transcripts.

First-run bootstrap events use the same logging policy. Dependency scans, install plans, background installer exit codes, authentication handoffs, and post-install probes must be captured as structured events with secrets redacted.

## What To Send For Diagnosis

When asking Codex to diagnose a Maestro issue, attach the latest `data/logs/*.ndjson` file and describe the visible symptom.

Each log line is standalone JSON with:

- `timestamp`
- `level`
- `category`
- `message`
- `context`
- `app`

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

## Redaction

The logger redacts common secret-shaped strings and sensitive keys such as `token`, `secret`, `password`, `credential`, `authorization`, `cookie`, and `api_key`. Credential logs may record presence flags, provider names, validation status, and redacted fingerprints, but never raw values. Even so, logs are operational artifacts and remain ignored by Git.

## Policy

- Logs stay inside the app folder.
- Logs are never committed.
- Runtime paths are covered by `.gitignore`.
- The logging detail level may be reduced only after the app reaches a stable enough release.
- UI verbosity changes what the operator sees, not what the diagnostic logger records during the unstable phase.
