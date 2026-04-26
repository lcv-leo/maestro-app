# Security Policy

## Supported Status

Maestro Editorial AI is in planning stage. No production version is supported yet.

## Reporting a Vulnerability

Do not open public issues for suspected secrets, credential leaks, or private editorial material. Report privately to the repository owner.

## Repository Rules

- Never commit `.env`, credentials, tokens, vaults, runtime session data, private protocols, drafts, evidence caches, or CLI transcripts.
- Use sanitized placeholders such as `<api_key_redacted>`.
- Treat CodeQL, secret scanning, and dependency alerts as release blockers until triaged.
