# ABNT Citation Engine

Status: implementation contract.
Date: 2026-04-26.

Maestro must apply the active editorial protocol as executable citation policy. The first profile is based on the attached Protocolo Editorial v1.10.0 and must treat ABNT formatting as a machine-checkable workflow, not as an optional style pass.

The private source protocol remains outside Git. Maestro stores operator-imported protocols locally and pins each session to a hash.

## Scope

The engine must support:

- ABNT NBR 10520:2023 citation formatting.
- ABNT NBR 6023 reference formatting.
- Direct quote, indirect quote, paraphrase, apud, footnote, and final-reference workflows.
- Mandatory locators for direct quotations.
- Detection of famous phrases in quotation marks as direct quotations.
- Compound surname preservation, including names that must not be truncated.
- Apparatus ordering required by the active protocol.
- Bibliographic quarantine for unverified or risky sources.
- Wikipedia and other prohibited-source handling according to the active protocol.
- Semantic diff of citation changes.

## Citation Inputs

Each citation candidate should be represented as structured data before formatting:

```json
{
  "schema_version": "citation.v1",
  "claim_id": "claim-001",
  "citation_type": "direct_quote | indirect_quote | paraphrase | apud | generic_mention",
  "author_display": "Sobrenome, Nome",
  "author_key": "SOBRENOME",
  "year": "2026",
  "locator": "p. 12",
  "source_id": "source-001",
  "source_access": "full_document_opened | excerpt_consulted | consolidated_memory | contextual_inference | unverified_hypothesis",
  "verification_status": "verified | needs_evidence | quarantined",
  "risk_if_wrong": "low | medium | high"
}
```

No direct quote may become publishable without a valid locator and a verified or explicitly operator-provided source.

## Outputs

The engine must generate:

- In-text citation text.
- Footnote text when the selected style requires it.
- Normalized ABNT reference.
- MainSite-compatible HTML.
- Pure Markdown.
- Citation audit table.
- Citation semantic diff.
- Machine-readable blockers.

## Maestro as Fourth Peer

Maestro is not only an orchestrator. It acts as a deterministic fourth peer:

- Claude, Codex, and Gemini produce editorial judgments.
- Maestro independently checks protocol gates, citation shape, link evidence, source freshness, quarantine status, and export structure.
- Final delivery requires AI trilateral unanimity and `MaestroPeer: READY`.

If Maestro finds a protocol/citation/evidence blocker, it must mark its own peer status as `NOT_READY` or `NEEDS_EVIDENCE` and create the next round even if all three AIs say `READY`.

## Required Blockers

Maestro must block publication when:

- A direct quotation lacks a locator.
- A quotation cannot be tied to a verified source.
- A source is in bibliographic quarantine.
- A prohibited source is used as citation support.
- A reference is missing required ABNT fields.
- A compound surname or canonical author name is malformed.
- A final reference exists without body use, or a body citation lacks final reference.
- A weak link is used only to satisfy a count.
- The public final text contains protocol self-reference.

## Future Tests

Golden fixtures must cover:

- Direct quote with page.
- Direct quote without page, blocked.
- Indirect quote.
- Paraphrase with source.
- Apud.
- Compound surnames.
- Online source with access date.
- Chapter/book/reference variants.
- Quarantined source.
- Prohibited source.
- Markdown export.
- MainSite HTML export.
