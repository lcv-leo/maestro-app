# Configuration Persistence

Status: implementation contract.
Date: 2026-04-26.

Maestro supports exactly three operator-selectable persistence modes for configuration, tokens, and API keys.

Every mode starts from a local, non-secret bootstrap file:

- `data/config/bootstrap.json`

This file is created automatically and tells Maestro where the rest of the configuration lives. It may store account IDs, mode names, env-var names, database names, and secret-store names. It must not store API tokens, provider API keys, OAuth refresh tokens, private keys, or raw secrets.

## Mode 1 - Local JSON

Everything is saved in local JSON files under ignored runtime paths.

Planned local files:

- `data/config/app-settings.json`
- `data/config/provider-profiles.json`
- `data/config/cloudflare-profile.json`
- `data/config/secrets.local.json`

Rules:

- This mode is portable and never requires Windows registry writes.
- The UI must warn that `secrets.local.json` contains raw secrets.
- Support bundles must exclude this file unless the operator explicitly exports a redacted copy.
- Git hygiene must keep `data/`, `secrets*.json`, `credentials*.json`, and local env files ignored.

## Mode 2 - Windows Env Var Hybrid

Only tokens and API keys are saved in Windows environment variables. All non-secret configuration remains in local JSON.

Secret examples:

- `MAESTRO_CLOUDFLARE_API_TOKEN`
- `MAESTRO_OPENAI_API_KEY`
- `MAESTRO_ANTHROPIC_API_KEY`
- `MAESTRO_GEMINI_API_KEY`
- `MAESTRO_DEEPSEEK_API_KEY`

JSON examples:

- Provider mode.
- Model pins.
- Cloudflare account ID and target database IDs.
- UI preferences.
- Runtime bootstrap status.

Rules:

- Default scope is `CurrentUser`.
- `LocalMachine` env vars require per-action administrator elevation and a clear preview.
- Maestro must re-open or refresh its environment after writing variables so the current process can see them.
- This mode is intentionally hybrid; it must never claim that all configuration is in env vars.

## Mode 3 - Cloudflare Remote

Everything is persisted in the operator's Cloudflare account.

One bootstrap exception is unavoidable: the Cloudflare API token needed to enter the Cloudflare account cannot live only inside that same Cloudflare account. On each app start, Maestro must get that initial token from one of these sources:

- Windows env var, preferably `MAESTRO_CLOUDFLARE_API_TOKEN`.
- A temporary operator entry in the local UI for that run.
- A future local encrypted vault bound to the Windows user.

The local `bootstrap.json` stores only the token source and env-var name, not the token value.

Cloudflare resources:

- D1 database: `maestro_db`.
- Secrets Store: `maestro` or another operator-approved store name.
- Optional future Worker broker: `maestro-secrets-broker`, used only if Maestro needs Cloudflare-side operations that consume secrets without exposing them to the desktop app.

Cloudflare Secrets Store values are write-only from the desktop app's perspective after creation. On startup, Maestro may recover `maestro_db` metadata and secret references such as `MAESTRO_OPENAI_API_KEY`, `MAESTRO_ANTHROPIC_API_KEY`, `MAESTRO_GEMINI_API_KEY`, and `MAESTRO_DEEPSEEK_API_KEY`, but the raw key material cannot be decrypted through the Cloudflare API. Direct local API calls therefore need a fresh operator-provided key, a Windows env var, or a future Cloudflare-side broker.

Planned D1 tables:

```sql
CREATE TABLE IF NOT EXISTS maestro_settings (
  key TEXT PRIMARY KEY,
  value_json TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS provider_profiles (
  provider TEXT PRIMARY KEY,
  transport_mode TEXT NOT NULL,
  model_pin TEXT,
  quota_policy_json TEXT,
  secret_ref TEXT,
  updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS cloudflare_profiles (
  profile_id TEXT PRIMARY KEY,
  account_id TEXT NOT NULL,
  d1_database_name TEXT NOT NULL,
  d1_database_id TEXT,
  secrets_store_id TEXT,
  main_site_database_name TEXT,
  main_site_table TEXT,
  updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS secret_mappings (
  logical_name TEXT PRIMARY KEY,
  provider TEXT NOT NULL,
  secret_store_id TEXT NOT NULL,
  secret_id TEXT NOT NULL,
  secret_version TEXT,
  updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS schema_migrations (
  id TEXT PRIMARY KEY,
  applied_at TEXT NOT NULL
);
```

Bootstrap flow:

1. Ask for Cloudflare Account ID and API token.
2. Verify token status through Cloudflare's token verification endpoint.
3. Confirm the account is reachable.
4. Create or locate D1 database `maestro_db`.
5. Apply idempotent D1 migrations.
6. Create or locate a Cloudflare Secrets Store.
7. Create or update required secrets.
8. Store only secret references in D1, never raw secret values.
9. Run a read/write verification against `maestro_db`.
10. Record the verified Cloudflare profile.

Important Secrets Store constraint:

- Cloudflare Secrets Store is designed so secret values are not read back in plaintext after they are stored.
- Maestro must treat Cloudflare secret reads as metadata/status reads only.
- If a later workflow needs to use a provider API key without keeping it locally, it must run through a Cloudflare-side broker or another Cloudflare product that can consume the secret securely.
- If a local CLI/API adapter needs the raw value, the operator must provide it for that local session or choose JSON/env-var persistence instead.
- Secret upsert must list Secrets Store entries with pagination and tolerate `secret_name_already_exists` by re-listing and patching the existing secret metadata/value reference instead of failing the whole save operation.

API policy:

- Cloudflare API is the primary path for D1 and Secrets Store provisioning.
- Wrangler is fallback only and must always be invoked as `wrangler@latest`.
- Wrangler fallback must not replace API readiness checks.

Required permission areas shown to the operator:

- Token verification.
- Account read.
- D1 read/edit for `maestro_db`.
- D1 read/edit for `bigdata_db.mainsite_posts` when MainSite publishing is enabled.
- Secrets Store read/edit or the current Cloudflare equivalent for creating stores and writing secrets.

## Selection Semantics

- JSON means all configuration and all secrets are local JSON.
- Env var means secrets are Windows env vars and all other configuration is JSON.
- Cloudflare means all configuration and secret references are remote; raw secrets are written to Cloudflare Secrets Store and not mirrored into local JSON.

The selected mode is part of diagnostics, but raw secret values are never logged.

## Official References

- Cloudflare D1 overview: https://developers.cloudflare.com/d1/
- Cloudflare D1 create database API: https://developers.cloudflare.com/api/operations/cloudflare-d1-create-database
- Cloudflare Secrets Store overview: https://developers.cloudflare.com/secrets-store/
- Cloudflare Secrets Store API: https://developers.cloudflare.com/api/resources/secrets_store/
- Cloudflare Secrets Store management: https://developers.cloudflare.com/secrets-store/manage-secrets/
