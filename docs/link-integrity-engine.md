# Link Integrity Engine

Status: implementation contract.
Date: 2026-04-26.

AI-generated texts often contain broken, invented, misleading, stale, or weak links. Maestro must treat link integrity as a central editorial gate and must take unresolved link problems back into cross-review.

## Responsibilities

The engine must:

- Extract links from Markdown, Markdown+HTML, TipTap HTML, shared-chat imports, PDF-derived text, and final exports.
- Preserve anchor text and surrounding citation/claim context.
- Normalize URLs without hiding meaningful changes.
- Validate with `HEAD`, `GET`, rendered fetch, and operator-assisted browsing when needed.
- Capture redirect chains, final URL, status, content type, content hash, timestamp, and error class.
- Detect hallucinated or confabulated links.
- Detect weak links that do not support the claim.
- Sanitize output links for MainSite rendering.
- Propose correction candidates and send them to cross-review.

## Link Classes

Every link receives one class:

- `verified_supports_claim`
- `verified_but_weak`
- `redirected_verified`
- `content_type_mismatch`
- `not_found`
- `forbidden`
- `auth_required`
- `captcha_required`
- `paywall`
- `timeout`
- `dns_error`
- `tls_error`
- `malformed`
- `suspected_hallucination`
- `quarantined`

Only `verified_supports_claim` and explicitly accepted `redirected_verified` links may remain in a publishable final text without a blocker note.

## Evidence Record

```json
{
  "schema_version": "link_evidence.v1",
  "link_id": "link-001",
  "source_artifact": "draft.md",
  "anchor_text": "artigo citado",
  "surrounding_text": "trecho curto ao redor do link",
  "original_url": "https://example.invalid/path",
  "normalized_url": "https://example.invalid/path",
  "final_url": "https://example.invalid/path",
  "redirect_chain": [],
  "http_status": 200,
  "content_type": "text/html",
  "sha256": "<content_hash>",
  "checked_at": "2026-04-26T00:00:00Z",
  "claim_supported": true,
  "classification": "verified_supports_claim",
  "correction_candidates": [],
  "cross_review_status": "not_needed | pending | accepted | rejected"
}
```

## Sanitization

For MainSite-compatible HTML, Maestro must:

- Preserve safe `http`, `https`, and `mailto` links.
- Reject `javascript:`, unsafe data URLs, malformed URLs, and suspicious control characters.
- Normalize internal LCV-family links according to the MainSite reader behavior.
- Add `target="_blank"` and `rel="noopener noreferrer"` to external non-YouTube links before save.
- Keep YouTube embed/link behavior compatible with PostEditor and PostReader.
- Re-run the sanitizer after any automatic correction.

## Correction Workflow

When a link fails:

1. Maestro records the failure.
2. Maestro searches for official or stronger alternatives.
3. Maestro proposes replacement, removal, or textual rewording.
4. Claude, Codex, and Gemini review the proposed change.
5. Maestro rechecks the accepted replacement mechanically.
6. The final text remains blocked until the link evidence is clean.

If no reliable replacement exists, the claim must be removed, rewritten without the link, or explicitly marked as unresolved in a non-publicable draft.

## Cross-Review Integration

Broken, invented, weak, or mismatched links create `NEEDS_EVIDENCE` unless the issue is purely syntactic and Maestro can repair it deterministically.

The cross-review prompt must include:

- Original link.
- Failure class.
- Anchor and surrounding text.
- Claim the link was supposed to support.
- Redirect/fetch evidence.
- Proposed correction candidates.
- Maestro's recommendation.

MaestroPeer remains `NOT_READY` or `NEEDS_EVIDENCE` until the accepted final link set passes mechanical validation.
