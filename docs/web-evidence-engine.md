# Web Evidence Engine

Status: implementation contract.
Date: 2026-04-26.

Maestro must compensate for the weak browsing/fetching capabilities of AI agents by collecting, checking, and packaging evidence itself.

The engine is for verification, citation support, and provenance. It should behave like a careful human researcher using a browser, with automation for repetitive checks and clear handoff to the operator whenever human interaction is required.

## Required Tools

Maestro should provide:

- HTTP fetch with `GET`, `HEAD`, redirect chain, status, headers, content type, final URL, timing, byte count, and content hash.
- curl-compatible request builder and reproducible command export, with secrets redacted.
- Web search connectors through official APIs or operator-configured providers.
- Browser-rendered fetch for pages that require JavaScript rendering.
- Human-assisted browsing window for CAPTCHA, login, consent, download confirmation, and other interactive gates.
- Link checker for shared-chat links, DOI URLs, catalog pages, PDF URLs, institutional repositories, and publisher pages.
- Link integrity ledger and correction pipeline as specified in `docs/link-integrity-engine.md`.
- PDF detection, download metadata, text extraction when lawful and technically possible, and source hashing.
- Evidence cache under ignored local runtime data, with TTL, revalidation, and stale-state markers.
- Structured evidence packs for Claude, Codex, and Gemini.

## Browser Identity, Profiles, and Human Interaction

Maestro runs on Windows 11+ and may use the system default browser in an operator-assisted mode:

1. Open a URL in the system default browser.
2. Let the operator use their normal browser/session when they have legitimate access.
3. Import a user-saved HTML, Markdown, PDF, screenshot, or copied text snapshot back into Maestro.
4. Record provenance as `operator_assisted_browser_capture`.

Maestro should also provide a small dedicated browser window inside the app. When CAPTCHA, login, cookie consent, download confirmation, or similar interaction appears, Maestro pauses the automated task and asks the operator to resolve it. After the operator confirms completion, Maestro resumes collection and records `human_resolved: true`.

Maestro must not silently read browser cookies, session stores, password stores, local profiles, or fingerprinting data. If a workflow needs the user's already-authenticated default browser, the safe path is to open the URL there and let the operator explicitly export/import the captured artifact.

Embedded rendering uses WebView2 with an app-owned user data folder under `./data/webview`, not the user's active browser profile.

## Access Policy

Automatic collection must:

- Respect robots.txt where Maestro acts as a crawler.
- Respect site terms, rate limits, authentication boundaries, and copyright constraints.
- Identify itself transparently in logs and evidence metadata.
- Avoid aggressive parallel crawling.
- Treat `401`, `403`, CAPTCHA, login wall, consent wall, download confirmation, paywall, and robots exclusion as evidence states that can trigger a human-assisted browser step or alternate-source search.
- Prefer official APIs, publisher pages, library catalogs, DOI registries, institutional repositories, and other authorized access paths.

When human action is required, Maestro records the state, opens the assisted window or default browser, waits for the operator, and then resumes from the resulting page/artifact.

## Evidence Record

Every fetched or captured item should persist:

```json
{
  "schema_version": "web_evidence.v1",
  "url": "https://example.invalid/source",
  "method": "GET",
  "access_mode": "http_fetch | rendered_fetch | official_api | operator_assisted_browser_capture",
  "status": 200,
  "final_url": "https://example.invalid/source",
  "content_type": "text/html",
  "sha256": "<content_hash>",
  "retrieved_at": "2026-04-26T00:00:00Z",
  "robots_state": "allowed | disallowed | unavailable | not_applicable",
  "copyright_state": "public | licensed | operator_provided | unknown",
  "interaction_state": "none | captcha_required | login_required | consent_required | download_confirmation | human_resolved",
  "cache_ttl": "P30D",
  "notes": []
}
```

## UI Requirements

The operator UI must show:

- Fetch queue.
- Search queue.
- Rendered-page queue.
- Blocked/challenge/paywall states.
- Evidence freshness.
- Reproducible curl command with redacted headers.
- Button to open a URL in a Maestro-assisted browser window.
- Button to open a URL in the default browser for operator capture.
- Import area for saved HTML, Markdown, PDF, image, screenshot, or text.
- Clear pause/resume controls when CAPTCHA, login, consent, or similar interaction needs the operator.

Raw credentials, cookies, authorization headers, and browser profile material must never appear in the UI, logs, exports, or Git-tracked files.
