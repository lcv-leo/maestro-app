# AI Provider Credentials

Status: implementation contract with v0.3.11 DeepSeek API peer integration.
Date: 2026-04-28.

Maestro must keep CLI orchestration and official API/SDK orchestration as first-class options. The operator should be able to use a subscription-backed CLI when that is convenient, or provide API credentials and pay through provider credits when that is the better path.

## Provider Modes

- `cli`: use the local `codex`, `claude`, and `gemini` CLIs.
- `api`: use official provider APIs/SDKs only.
- `hybrid`: prefer API/SDK for agents with validated credentials and fall back to CLI only when explicitly allowed by policy.

No mode changes the convergence rule. Claude, Codex/OpenAI, Gemini, DeepSeek, and MaestroPeer still need unanimous `READY` in the same accepted round before final delivery when those peers are active in the session.

## Settings Fields

The settings screen must provide secure credential fields for:

- OpenAI / Codex: API key, optional organization ID, optional project ID, model pin, request budget, and SDK/API route.
- Anthropic / Claude: API key, workspace label, API version pin, model pin, request budget, and direct/partner-platform route.
- Google / Gemini: Gemini Developer API key, optional Vertex AI project/location, model pin, request budget, and backend selector.
- DeepSeek: API key, model pin, request budget, and direct DeepSeek API route.

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

Implemented through v0.3.11:

- The settings screen has explicit `Salvar APIs` and `Verificar APIs` actions.
- Local JSON persistence writes `data/config/ai-providers.json`, which remains under ignored runtime data.
- Verification calls official model-list endpoints for OpenAI, Anthropic, Gemini, and DeepSeek and reports provider-level status without logging raw keys.
- Network-error rendering strips request URLs before messages reach the UI/logs, so query-string API keys are not echoed when a provider request fails before a response is received.
- DeepSeek can generate drafts, review drafts, and produce revisions through the direct API path. At runtime, Maestro asks the authenticated `/models` endpoint which models are available and selects the strongest supported entry, preferring `deepseek-v4-pro` when exposed. The model can be overridden with `MAESTRO_DEEPSEEK_MODEL` or `CROSS_REVIEW_DEEPSEEK_MODEL`.
- Windows env-var read is active for provider keys; write UX is still pending.
- Cloudflare Secrets Store persistence writes provider keys and reloads secret references, but raw values remain non-readable from the desktop app by design.

Still pending:

- Windows env-var write UX for provider keys.
- Cloudflare-side broker or AI Gateway integration for consuming Secrets Store values without exposing them back to the desktop app.
- Full SDK/API orchestration for all peers as an alternative to CLI editorial sessions.

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
- `MAESTRO_DEEPSEEK_API_KEY`
- `MAESTRO_GOOGLE_VERTEX_PROJECT`
- `MAESTRO_GOOGLE_VERTEX_LOCATION`

Machine-wide environment variables require administrator elevation, a command preview, and a post-action verification step. Current-user variables are preferred.

## Official API Notes

Implementation must re-check current provider documentation before coding each adapter because APIs, SDKs, model names, and authentication rules change often.

Current planning references:

- OpenAI API keys use bearer authentication and may include organization/project headers for multi-project accounts.
- Anthropic requests require `x-api-key`, `anthropic-version`, and JSON content headers; official SDKs manage those headers.
- Gemini API requests use `x-goog-api-key` for the Gemini Developer API; the Google Gen AI SDK can target both Gemini Developer API and Vertex AI.
- DeepSeek supports OpenAI-compatible authentication with bearer tokens at `https://api.deepseek.com`; the implemented direct peer uses `/models` for verification/model selection and `/chat/completions` for editorial calls. In the current authenticated probe, `/models` exposes `deepseek-v4-pro` and `deepseek-v4-flash`.

Official documentation:

- OpenAI API authentication: https://platform.openai.com/docs/api-reference/authentication
- Anthropic API authentication: https://docs.anthropic.com/en/api/getting-started
- Gemini API keys: https://ai.google.dev/tutorials/setup
- Google Gen AI SDK / Gemini Developer API and Vertex AI: https://ai.google.dev/gemini-api/docs/migrate-to-cloud
- DeepSeek API quick start: https://api-docs.deepseek.com/
- DeepSeek model list endpoint: https://api-docs.deepseek.com/api/list-models

## Cross-Review Use

Provider credential status is part of session readiness. A peer is unavailable if neither its CLI path nor its API path is validated for the configured session.

Maestro must record which path produced each agent response:

```json
{
  "provider": "openai | anthropic | google | deepseek",
  "agent": "codex | claude | gemini",
  "transport": "cli | api_sdk",
  "model_pin": "provider-model-id",
  "credential_ref": "local-vault-reference",
  "request_id": "provider-request-id-or-null"
}
```

`credential_ref` is an opaque local reference and must never be a real key.
