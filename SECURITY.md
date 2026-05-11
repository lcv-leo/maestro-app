# Security Policy

## Supported Status

Maestro Editorial AI is in planning stage. No production version is supported yet.

## Reporting a Vulnerability

Do not open public issues for suspected secrets, credential leaks, private editorial material, authentication bypasses, or executable-supply-chain issues.

Use GitHub private vulnerability reporting for this repository:

<https://github.com/LCV-Ideas-Software/maestro-app/security/advisories/new>

If GitHub blocks private reporting for your account, contact the repository owner privately through the LCV Ideas & Software GitHub organization. Include:

- affected version or commit SHA;
- operating system and installation method;
- minimal reproduction steps;
- expected impact;
- whether any credentials, session files, or editorial drafts may have been exposed.

We aim to acknowledge valid reports within 7 days while the project remains pre-production.

## Repository Rules

- Never commit `.env`, credentials, tokens, vaults, runtime session data, private protocols, drafts, evidence caches, or CLI transcripts.
- Use sanitized placeholders such as `<api_key_redacted>`.
- Treat CodeQL, secret scanning, and dependency alerts as release blockers until triaged.
