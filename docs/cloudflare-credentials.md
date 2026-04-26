# Cloudflare Credentials

Status: implementation contract.
Date: 2026-04-26.

Maestro must provide a settings screen for Cloudflare credentials used by D1 import/export and MainSite publishing workflows. All Cloudflare D1 operations use the Cloudflare API as the primary path. Wrangler is a fallback for API outage, provider/API drift, diagnostics, and operator-approved recovery. When Wrangler fallback is needed, Maestro must invoke `wrangler@latest` and may auto-authorize the update/install step associated with that fallback.

## Required Fields

- Cloudflare Account ID.
- Cloudflare API Token.
- Target D1 database name or ID, defaulting to `bigdata_db` when configured by the operator.
- Target table, defaulting to `mainsite_posts` for MainSite publishing.

The API token field must be masked, never logged, never exported, and never committed. Account IDs may appear in diagnostics only when the operator chooses to include them.

## Token Validation

When the operator enters a token, Maestro must validate it before enabling Cloudflare operations:

1. Check token syntax and redaction behavior locally.
2. Call Cloudflare's `/user/tokens/verify` endpoint.
3. Confirm the returned token status is active.
4. Confirm the configured account is reachable.
5. Confirm D1 access by listing or resolving the target D1 database.
6. Confirm table access by running a safe read probe against `mainsite_posts`.
7. Confirm write capability only through an explicit dry-run or operator-approved test transaction.
8. Confirm Wrangler fallback separately when installed, without treating Wrangler readiness as a replacement for API readiness.

Validation must produce a clear status:

- `token_active`
- `account_unreachable`
- `d1_read_missing`
- `d1_write_missing`
- `database_not_found`
- `table_not_found`
- `write_probe_not_authorized`
- `ready`

## Required Cloudflare Permissions

Minimum guidance shown to the operator:

- For read/import: Account-level `D1 Read`.
- For write/export/update: Account-level `D1 Read` plus `D1 Edit` or current Cloudflare API equivalent `D1 Write`.
- If Maestro later manages Pages or Workers resources directly, it must request those permissions separately and explain why.

Maestro should fetch the current Cloudflare permission group list when possible because Cloudflare names can evolve over time.

## Storage

The operator chooses how Maestro persists or reads Cloudflare credentials.

Supported modes:

- Encrypted local vault: preferred stable path, stored under ignored runtime data.
- Windows environment variables: read existing variables or write them with operator approval. Prefer `CurrentUser`; `LocalMachine` requires administrator elevation and a per-action UAC boundary.
- Local JSON config: allowed only after a clear warning that the file contains secrets and must remain ignored, backed up carefully, and never shared.

Suggested environment variable names:

- `MAESTRO_CLOUDFLARE_ACCOUNT_ID`
- `MAESTRO_CLOUDFLARE_API_TOKEN`
- `MAESTRO_CLOUDFLARE_D1_DATABASE`
- `MAESTRO_CLOUDFLARE_D1_TABLE`

Never commit `.env`, local JSON credential files, raw tokens, or generated support bundles containing secrets. Never include the raw token in logs, support bundles, session minutes, Markdown exports, cross-review prompts, or shell command arguments when an environment or stdin handoff is available.

## UI Requirements

The settings screen must show:

- Account ID input.
- API token secure input.
- Token status.
- Required permissions checklist.
- D1 database/table probe results.
- Button to verify token.
- Storage mode selector: encrypted vault, Windows env var, or local JSON.
- Button to open Cloudflare token creation docs/dashboard.
- Button to clear local credentials.
- Last verification timestamp.

If a token is under-scoped, Maestro must instruct the operator which permission is missing and which operation is blocked.

## Official References

- Cloudflare token verification: https://developers.cloudflare.com/api/resources/accounts/subresources/tokens/methods/verify/
- Cloudflare API token permissions: https://developers.cloudflare.com/fundamentals/api/reference/permissions/
- Cloudflare D1 API: https://developers.cloudflare.com/api/resources/d1/models/d1/
