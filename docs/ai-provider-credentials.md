# AI Provider Credentials

Status: implementation contract.
Date: 2026-04-26.

Maestro must keep CLI orchestration and official API/SDK orchestration as first-class options. The operator should be able to use a subscription-backed CLI when that is convenient, or provide API credentials and pay through provider credits when that is the better path.

## Provider Modes

- `cli`: use the local `codex`, `claude`, and `gemini` CLIs.
- `api`: use official provider APIs/SDKs only.
- `hybrid`: prefer API/SDK for agents with validated credentials and fall back to CLI only when explicitly allowed by policy.

No mode changes the convergence rule. Claude, Codex/OpenAI, Gemini, and MaestroPeer still need unanimous `READY` in the same accepted round before final delivery.

## Settings Fields

The settings screen must provide secure credential fields for:

- OpenAI / Codex: API key, optional organization ID, optional project ID, model pin, request budget, and SDK/API route.
- Anthropic / Claude: API key, workspace label, API version pin, model pin, request budget, and direct/partner-platform route.
- Google / Gemini: Gemini Developer API key, optional Vertex AI project/location, model pin, request budget, and backend selector.

The UI must also keep each provider's CLI path/version/auth status visible because CLI and API operation are independent readiness surfaces.

## Validation

When a credential is entered, Maestro must validate in layers:

1. Local syntax check and redaction test.
2. Provider reachability.
3. Authentication check against the official API.
4. Model availability check for the configured model pin.
5. Rate-limit, quota, or billing-readiness check when the provider exposes it.
6. A minimal non-destructive smoke request after explicit operator approval.

Validation statuses:

- `not_configured`
- `redaction_failed`
- `auth_failed`
- `model_unavailable`
- `quota_unavailable`
- `rate_limited`
- `ready`

If a provider credential is invalid or underfunded, Maestro must explain which provider path is blocked and whether the CLI path can still satisfy that peer.

## Security

- Never store raw provider API keys in Git-tracked files.
- Never include provider keys in cross-review prompts, session minutes, Markdown exports, support bundles, or raw UI activity.
- Store keys only in the ignored local encrypted vault once implemented.
- During early development, any fallback local config must remain ignored and clearly marked unsafe for production.
- Logs may include provider name, key presence, validation status, request IDs, and redacted fingerprints only.
- Do not print provider keys into shell commands or process arguments when an SDK or stdin/config-file handoff can avoid it.

## Storage Options

The operator chooses one of the three persistence modes defined in `docs/configuration-persistence.md`:

- Local JSON: all provider credentials and configuration are stored in ignored JSON files.
- Windows environment variables: provider API keys are stored in user-scope env vars; model pins, route preferences, and non-secret settings remain in JSON.
- Cloudflare: provider profile settings are stored in D1 `maestro_db`; raw API keys are written to Cloudflare Secrets Store and D1 stores only secret references.

Cloudflare mode caveat:

- Secrets Store values are not read back in plaintext. Local desktop adapters that need raw provider keys must either receive a fresh operator-provided value for that session or route through a Cloudflare-side broker that can consume the secret without exposing it to Maestro.

Suggested user-scope Windows environment variable names:

- `MAESTRO_OPENAI_API_KEY`
- `MAESTRO_OPENAI_ORG_ID`
- `MAESTRO_OPENAI_PROJECT_ID`
- `MAESTRO_ANTHROPIC_API_KEY`
- `MAESTRO_GEMINI_API_KEY`
- `MAESTRO_GOOGLE_VERTEX_PROJECT`
- `MAESTRO_GOOGLE_VERTEX_LOCATION`

Machine-wide environment variables require administrator elevation, a command preview, and a post-action verification step. Current-user variables are preferred.

## Official API Notes

Implementation must re-check current provider documentation before coding each adapter because APIs, SDKs, model names, and authentication rules change often.

Current planning references:

- OpenAI API keys use bearer authentication and may include organization/project headers for multi-project accounts.
- Anthropic requests require `x-api-key`, `anthropic-version`, and JSON content headers; official SDKs manage those headers.
- Gemini API requests use `x-goog-api-key` for the Gemini Developer API; the Google Gen AI SDK can target both Gemini Developer API and Vertex AI.

Official documentation:

- OpenAI API authentication: https://platform.openai.com/docs/api-reference/authentication
- Anthropic API authentication: https://docs.anthropic.com/en/api/getting-started
- Gemini API keys: https://ai.google.dev/tutorials/setup
- Google Gen AI SDK / Gemini Developer API and Vertex AI: https://ai.google.dev/gemini-api/docs/migrate-to-cloud

## Cross-Review Use

Provider credential status is part of session readiness. A peer is unavailable if neither its CLI path nor its API path is validated for the configured session.

Maestro must record which path produced each agent response:

```json
{
  "provider": "openai | anthropic | google",
  "agent": "codex | claude | gemini",
  "transport": "cli | api_sdk",
  "model_pin": "provider-model-id",
  "credential_ref": "local-vault-reference",
  "request_id": "provider-request-id-or-null"
}
```

`credential_ref` is an opaque local reference and must never be a real key.
