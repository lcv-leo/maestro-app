# Import, Export, and Cloudflare D1 Plan

Status: implementation contract.
Date: 2026-04-26.

## Shared Chat Links

Maestro must classify and import shared chat snapshots from the three provider web apps:

- ChatGPT: `https://chatgpt.com/share/<conversation-id>`.
- Gemini: `https://g.co/gemini/share/...` and canonical Gemini shared-chat URLs.
- Claude: Claude shared chat links created through the Claude sharing flow.

Import is evidence-oriented, not blind scraping:

1. Normalize the URL and provider.
2. Fetch the public snapshot through a browser-capable fetch path when static HTML is insufficient.
3. Extract prompt, response text, artifacts when visible, timestamp hints, and source URL.
4. Convert to normalized Markdown plus a JSON provenance record.
5. Store import evidence under ignored local session data.
6. Never treat a shared chat as a verified source for factual claims; it is an input artifact.

If a provider changes the share page structure, the importer must fail with a diagnostic event rather than fabricating content.

## File Formats

Required read and write paths:

- Pure Markdown.
- Markdown plus trusted HTML blocks.
- PDF import for text extraction and PDF export for final delivery.
- MainSite-compatible HTML through the PostEditor parity module.

Markdown and PDF conversions must preserve provenance metadata separately from the public final text.

## Web Evidence

Shared-chat imports and source verification depend on the Web Evidence Engine in `docs/web-evidence-engine.md`.

If a provider or website requires human interaction, Maestro must pause, open an assisted browser window or the system default browser, let the operator resolve CAPTCHA/login/consent/download prompts, and then import or continue from the resulting artifact. It must not use hidden browser-profile access or cookie extraction.

## Cloudflare D1

Target:

```text
bigdata_db.mainsite_posts
```

Maestro may read, import, export, insert, and update records, but the stable write path is gated by:

- Local credentials stored only in ignored runtime vault/config files.
- Cloudflare API as the primary execution path for every D1 operation.
- Wrangler as a fallback execution path only when the API route is unavailable, blocked by tooling drift, or explicitly selected for diagnosis by the operator.
- Wrangler fallback must invoke `wrangler@latest`; Maestro may automatically authorize the Wrangler latest update/install once the operator has approved use of the fallback CLI path.
- Operator confirmation before any remote write.
- Dry-run preview containing SQL intent, affected row, and sanitized HTML diff.
- PostEditor parity output.
- MainSite sanitizer pass.
- `PostReader` compatibility fixtures.

For a local Windows desktop app, D1 access must use Cloudflare's API first. Wrangler is installed and managed because it is useful for fallback, diagnostics, and operator-visible parity with Cloudflare's CLI, but it is not the default D1 write/read path. When that fallback is needed, Maestro must run the latest Wrangler surface (`wrangler@latest`) rather than trusting a stale global install. API tokens, account IDs, database IDs, and Cloudflare credentials must never be committed.
