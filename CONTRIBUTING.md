# Contributing to Maestro Editorial AI

Maestro is in planning stage. Contributions are not open yet.

## Engineering Rules

- Treat GitHub Secret Scanning, Code Scanning, CodeQL, and Dependabot as active gates.
- Do not commit private editorial protocols, user drafts, evidence caches, CLI transcripts, credentials, tokens, `.env` files, or local app data.
- Use sanitized placeholders such as `<api_key_redacted>`.
- Keep source changes aligned with the architecture plan in `docs/architecture-plan-v0.1.md`.
- Runtime data belongs under ignored `data/` paths, never in tracked fixtures.

## Before Any Pull Request

- Run local validation for the touched package once code exists.
- Confirm no secret-shaped strings were introduced.
- Confirm public-facing docs do not mention private protocol contents unless explicitly approved.
- Update `CHANGELOG.md` when behavior, security posture, release process, or protocol handling changes.
