# AI Provider Credentials

Status: implementation contract with direct API peer execution, per-session cost controls, Grok/xAI peer support, and provider prompt-cache policy telemetry through v0.5.19.
Date: 2026-05-10.

Maestro must keep CLI orchestration and official API/SDK orchestration as first-class options. The operator should be able to use a subscription-backed CLI when that is convenient, or provide API credentials and pay through provider credits when that is the better path.

## Provider Modes

- `cli`: use the local `codex`, `claude`, and `gemini` CLIs.
- `api`: use official provider APIs/SDKs only.
- `hybrid`: prefer API/SDK for agents with validated credentials and fall back to CLI only when explicitly allowed by policy.

No mode changes the convergence rule. Claude, Codex/OpenAI, Gemini, DeepSeek, Grok, and MaestroPeer still need unanimous `READY` in the same accepted round before final delivery when those peers are active in the session.

From `v0.5.16`, the operator can select 1 to 5 active AI peers per session. Unselected peers are not called and do not count toward that session's unanimity gate.

## Settings Fields

The settings screen provides secure credential fields and UI-owned tariff rows for:

- OpenAI / Codex: API key and input/output USD per 1M tokens.
- Anthropic / Claude: API key and input/output USD per 1M tokens.
- Google / Gemini: Gemini Developer API key and input/output USD per 1M tokens.
- DeepSeek: API key and input/output USD per 1M tokens.
- Grok / xAI: API key and input/output USD per 1M tokens.

Per-provider model pins and organization/project routing are future configuration fields; current execution resolves models dynamically from authenticated provider model-list endpoints.

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

Implemented through v0.3.13:

- The settings screen has explicit `Salvar APIs` and `Verificar APIs` actions.
- Local JSON persistence writes `data/config/ai-providers.json`, which remains under ignored runtime data.
- Verification calls official model-list endpoints for OpenAI, Anthropic, Gemini, and DeepSeek and reports provider-level status without logging raw keys.
- Network-error rendering strips request URLs before messages reach the UI/logs, so query-string API keys are not echoed when a provider request fails before a response is received.
- DeepSeek, OpenAI/Codex, Anthropic/Claude, and Google/Gemini can generate drafts, review drafts, and produce revisions through direct provider APIs. At runtime, Maestro asks each authenticated model-list endpoint which models are available and selects the strongest supported entry for that provider. DeepSeek still honors `MAESTRO_DEEPSEEK_MODEL` or `CROSS_REVIEW_DEEPSEEK_MODEL` when set.
- Grok/xAI runs API-only in API and hybrid modes. CLI mode disables Grok instead of pretending a local CLI transport exists.
- Optional per-session USD budgets are enforced against observed direct API usage. The limit remains one session-level value; Maestro never creates per-model budgets or silently drops a selected peer to stay under budget.
- Provider tariffs are UI-owned configuration. The operator maintains input/output USD per 1M tokens in `Configuracoes > Agentes via API > Tabela de tarifas`; there is no env-var fallback for cost rates. Any peer that will run through a direct provider API is blocked with a friendly message until both tariff fields for that provider are configured.
- CLI-backed peers expose no reliable per-call token usage to Maestro yet. Their cost is displayed as unknown/subscription and does not decrement the optional USD budget.
- Windows env-var read is active for provider keys; write UX is still pending.
- Cloudflare Secrets Store persistence writes provider keys and reloads secret references, but raw values remain non-readable from the desktop app by design.

Still pending:

- Windows env-var write UX for provider keys.
- Cloudflare-side broker or AI Gateway integration for consuming Secrets Store values without exposing them back to the desktop app.
- Optional UI model pinning per provider. Current direct API execution resolves models dynamically from each provider's model-list endpoint, with conservative fallbacks.
- Cloudflare-side broker or AI Gateway cost telemetry for hosted/remote execution paths.

## Prompt Cache Policy

Prompt caching is used only when it can reduce paid input cost without weakening the editorial protocol, disabling thinking, or changing the selected model. Cache configuration never stores raw prompts, API keys, or protocol text in public files.

Implemented through `v0.5.19`:

- OpenAI/Codex direct Responses calls send a deterministic `prompt_cache_key`. For extended-cache-capable OpenAI models selected by Maestro, the request also sends `prompt_cache_retention: "24h"`. Other OpenAI models keep the key and omit the retention override so the provider default applies.
- Anthropic/Claude direct Messages calls send the stable `system` prompt as a text block marked with `cache_control: { "type": "ephemeral" }`, which enables prompt caching with the provider's default short retention when the stable prefix is long enough. Maestro reads `cache_creation_input_tokens` and `cache_read_input_tokens` from the response usage object.
- DeepSeek uses the provider's automatic disk cache. Maestro does not add non-standard request fields; it records `prompt_cache_hit_tokens` and `prompt_cache_miss_tokens` when DeepSeek returns them.
- Grok/xAI direct Responses calls send a deterministic `prompt_cache_key` and parse cached-token usage fields when present.
- Gemini keeps the GenerateContent payload thinking-preserving. Explicit Gemini cached-content resources are not forced from the desktop runner because the current quality requirement is to preserve thinking mode; Maestro records provider cache usage if `usageMetadata.cachedContentTokenCount` is returned.
- Each API peer writes non-secret cache policy metadata to NDJSON and to `data/sessions/<run>/cache-manifest.ndjson`: provider, model, role, cache mode, cache key hash, retention label, stable-prefix character count, and prompt character count.
- Each successful API artifact includes cache mode, key hash, control status, retention, cached input tokens, hit tokens, miss tokens, read tokens, and creation tokens where known.

The cache key hash is derived from provider, model, role, agent name, and the stable system prompt. It is meant for diagnostics and provider routing only; it is not a secret and it is not enough to reconstruct the prompt.

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
- `MAESTRO_GROK_API_KEY`
- `MAESTRO_GOOGLE_VERTEX_PROJECT`
- `MAESTRO_GOOGLE_VERTEX_LOCATION`

Machine-wide environment variables require administrator elevation, a command preview, and a post-action verification step. Current-user variables are preferred.

## Official API Notes

Implementation must re-check current provider documentation before coding each adapter because APIs, SDKs, model names, and authentication rules change often.

Current planning references:

- OpenAI API keys use bearer authentication and may include organization/project headers for multi-project accounts.
- OpenAI direct calls use the Responses API at `/v1/responses`, bearer auth, `input`, `instructions`, `max_output_tokens`, and response `usage.input_tokens`/`usage.output_tokens` for cost accounting.
- Anthropic direct calls use Messages API at `/v1/messages` with `x-api-key`, `anthropic-version`, `model`, `max_tokens`, `system`, and `messages`; response `usage.input_tokens`/`usage.output_tokens` feeds the cost ledger.
- Gemini direct calls use `models/{model}:generateContent` with API-key auth, `contents`, `systemInstruction`, and `generationConfig.maxOutputTokens`; response `usageMetadata.promptTokenCount`/`candidatesTokenCount` feeds the cost ledger.
- Grok/xAI direct calls use the OpenAI-compatible Responses API at `https://api.x.ai/v1/responses`, bearer auth, `input`, `max_output_tokens`, `store: false`, and `prompt_cache_key`.
- Direct API attachments are provider-shaped instead of text-only: OpenAI receives supported images as `input_image` and supported documents as `input_file` with base64 data URLs; Anthropic receives supported images and PDFs as base64 content blocks; Gemini receives supported media/documents as `inline_data` parts.
- Attachment types that are not natively supported by the selected provider, or that exceed the native API inline size cap, remain available through the session manifest and bounded text previews. Native attachment payload size is included in the conservative pre-call cost projection.
- The session UI mirrors this as a pre-run per-provider prediction, so mixed support is visible before invocation instead of collapsed into a single native/manifest label.
- DeepSeek supports OpenAI-compatible authentication with bearer tokens at `https://api.deepseek.com`; the implemented direct peer uses `/models` for verification/model selection and `/chat/completions` for editorial calls. In the current authenticated probe, `/models` exposes `deepseek-v4-pro` and `deepseek-v4-flash`.

Official documentation:

- OpenAI API authentication: https://platform.openai.com/docs/api-reference/authentication
- OpenAI Responses API: https://platform.openai.com/docs/api-reference/responses/create
- Anthropic API authentication: https://docs.anthropic.com/en/api/getting-started
- Anthropic Messages API: https://docs.anthropic.com/en/api/messages
- Gemini API keys: https://ai.google.dev/tutorials/setup
- Gemini GenerateContent API: https://ai.google.dev/api/generate-content
- Google Gen AI SDK / Gemini Developer API and Vertex AI: https://ai.google.dev/gemini-api/docs/migrate-to-cloud
- DeepSeek API quick start: https://api-docs.deepseek.com/
- DeepSeek model list endpoint: https://api-docs.deepseek.com/api/list-models
- xAI Responses API / prompt caching: https://docs.x.ai/docs/guides/prompt-caching

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
