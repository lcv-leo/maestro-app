// Modulo: src-tauri/src/editorial_prompts.rs
// Descricao: Editorial agent specs (CLI args + spec table) and prompt builders
// (draft/review/revision) extracted from lib.rs in v0.3.25 per
// `docs/code-split-plan.md` migration step 5.
//
// What's here (12 functions):
//   - `claude_args`, `codex_args`, `gemini_args`, `deepseek_args` — argv
//     templates for each peer CLI; called as `(spec.args)()` to materialize
//     a fresh `Vec<String>` per spawn.
//   - `editorial_agent_specs` — 4-entry vector of `EditorialAgentSpec` keyed
//     by peer name.
//   - `resolve_initial_agent_key` — normalizes operator's free-form initial
//     agent string into one of {claude, codex, gemini, deepseek} with an
//     optional warning string for unrecognized inputs.
//   - `ordered_editorial_agent_specs` — places the chosen first key at the
//     head of the spec list, others follow in declaration order.
//   - `build_draft_prompt` — markdown prompt template for the redactor
//     opening the editorial session.
//   - `build_review_prompt` — review prompt with the MAESTRO_STATUS contract.
//   - `build_revision_prompt` — revision prompt that also embeds peer review
//     excerpts read from each agent's artifact.
//
// What stays in lib.rs (consumed via `pub(crate)` imports):
//   - `EditorialAgentSpec` struct (pub(crate) since v0.3.0; v0.3.25 upgrades
//     its `name`/`command`/`args` fields to pub(crate) so the migrated agent
//     spec table can construct values).
//   - `EditorialSessionRequest` struct (v0.3.25 upgrades to pub(crate) +
//     pub(crate) fields so the prompt builders can read session_name/prompt/
//     protocol_text).
//   - `EditorialAgentResult`, `extract_stdout_block`, `read_text_file`,
//     `sanitize_text` — all pub(crate) prior to v0.3.25.
//
// v0.3.25 is a pure move: every signature, format string and template body
// is identical to the v0.3.24 lib.rs source (commit cdb509f).

use std::path::Path;

use crate::{
    extract_stdout_block, extract_tagged_block, read_text_file, sanitize_text,
    EditorialAgentResult, EditorialAgentSpec, EditorialSessionRequest,
};

pub(crate) fn claude_args() -> Vec<String> {
    vec![
        "--print".to_string(),
        "--input-format".to_string(),
        "text".to_string(),
        "--output-format".to_string(),
        "text".to_string(),
        "--permission-mode".to_string(),
        "dontAsk".to_string(),
    ]
}

pub(crate) fn codex_args() -> Vec<String> {
    vec![
        "--ask-for-approval".to_string(),
        "never".to_string(),
        "exec".to_string(),
        "--skip-git-repo-check".to_string(),
        "--sandbox".to_string(),
        "read-only".to_string(),
        "--color".to_string(),
        "never".to_string(),
        "Read the complete <stdin> block provided by Maestro and follow its instructions exactly."
            .to_string(),
    ]
}

pub(crate) fn gemini_args() -> Vec<String> {
    vec![
        "--prompt".to_string(),
        "Read the complete stdin payload and follow Maestro's instructions exactly.".to_string(),
        "--output-format".to_string(),
        "text".to_string(),
        "--approval-mode".to_string(),
        "yolo".to_string(),
        "--skip-trust".to_string(),
    ]
}

pub(crate) fn deepseek_args() -> Vec<String> {
    Vec::new()
}

pub(crate) fn grok_args() -> Vec<String> {
    Vec::new()
}

pub(crate) fn editorial_agent_specs() -> Vec<EditorialAgentSpec> {
    vec![
        EditorialAgentSpec {
            key: "claude",
            name: "Claude",
            command: "claude",
            args: claude_args,
        },
        EditorialAgentSpec {
            key: "codex",
            name: "Codex",
            command: "codex",
            args: codex_args,
        },
        EditorialAgentSpec {
            key: "gemini",
            name: "Gemini",
            command: "gemini",
            args: gemini_args,
        },
        EditorialAgentSpec {
            key: "deepseek",
            name: "DeepSeek",
            command: "deepseek-api",
            args: deepseek_args,
        },
        EditorialAgentSpec {
            key: "grok",
            name: "Grok",
            command: "grok-api",
            args: grok_args,
        },
    ]
}

pub(crate) fn resolve_initial_agent_key(value: Option<&str>) -> (&'static str, Option<String>) {
    let Some(value) = value else {
        return ("claude", None);
    };
    let normalized = value.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "claude" | "anthropic" => ("claude", None),
        "codex" | "openai" | "chatgpt" => ("codex", None),
        "gemini" | "google" => ("gemini", None),
        "deepseek" | "deepseek-api" => ("deepseek", None),
        "grok" | "xai" | "grok-api" => ("grok", None),
        "" => ("claude", None),
        _ => ("claude", Some(sanitize_text(value, 80))),
    }
}

pub(crate) fn ordered_editorial_agent_specs(first_key: &str) -> Vec<EditorialAgentSpec> {
    let specs = editorial_agent_specs();
    let mut ordered = specs
        .iter()
        .copied()
        .filter(|spec| spec.key == first_key)
        .collect::<Vec<_>>();
    ordered.extend(specs.into_iter().filter(|spec| spec.key != first_key));
    ordered
}

pub(crate) fn build_draft_prompt(
    request: &EditorialSessionRequest,
    run_id: &str,
    evidence_block: &str,
) -> String {
    format!(
        r#"# Maestro Editorial AI - Internal Draft Request

Run: `{run_id}`
Session: {}

## Language Contract

- Internal coordination between agents/peers MUST be written in en_US.
- The operator-facing deliverable MUST be written in Brazilian Portuguese (pt_BR).
- Keep protocol markers exactly as specified, including `MAESTRO_STATUS` when applicable.

## Role Contract

You are the drafter selected to open the editorial session.
In this cycle you act as petitioner/drafter: you submit a complete text to the editorial panel, but you never vote as reviewer of your own text.
Read the full editorial protocol before writing.
Produce a complete Markdown draft for the operator request.
Do not create local files. Write the entire answer only to stdout.
Do not invent links. If evidence is missing, mark it explicitly as `[EVIDENCIA_PENDENTE]`.

## Operator Request

{}

## Full Editorial Protocol

```markdown
{}
```
{}
"#,
        sanitize_text(&request.session_name, 200),
        request.prompt,
        request.protocol_text,
        evidence_block
    )
}

#[cfg(test)]
pub(crate) fn build_review_prompt(
    request: &EditorialSessionRequest,
    run_id: &str,
    round: usize,
    draft: &str,
    draft_author_key: &str,
    previous_blocking_notes: &str,
    evidence_block: &str,
) -> String {
    let previous_blocking_notes = if previous_blocking_notes.trim().is_empty() {
        "No prior blocking objections are recorded for this review cycle.".to_string()
    } else {
        previous_blocking_notes.trim().to_string()
    };
    format!(
        r#"# Maestro Editorial AI - Internal Peer Review

Run: `{run_id}`
Session: {}
Review round: `{round}`

## Language Contract

- Internal peer-review notes MUST be written in en_US.
- The reviewed deliverable itself is intended for the operator in Brazilian Portuguese (pt_BR).
- Keep the first line status marker exactly as required.

## Collegiate Review Contract

- Text author/petitioner under review: `{}`.
- You act only as an independent reviewer of the editorial panel.
- The author of the current draft/revision must never vote as reviewer of that same text.
- If this call indicates that you are the text author, return `MAESTRO_STATUS: NOT_READY` and record `SELF_REVIEW_BLOCKED`; the backend should have blocked that case before the call.
- `READY` means an affirmative vote with no blocking objection.
- `NOT_READY` means a vote with at least one concrete blocking objection.

## Change Boundary Contract

- Round 1: perform a full protocol review.
- Round 2 and later: focus on unresolved blocking objections from prior rounds and materially new regressions introduced by the latest revision.
- Do NOT reopen or rewrite content that has already been accepted, unless the latest revision changed it or you identify a direct protocol-breaking blocker.
- Do NOT ask the reviser to "read everything again" or broadly rewrite approved sections. State only concrete blocking corrections.
- If a possible concern is stylistic, optional, or outside the blocking scope, mark it as `OUT_OF_SCOPE` and do not use it to justify `NOT_READY`.

## Approved Content Lock

Approved content is locked. A locked passage can be reopened only when one of these hard gates is true:

1. The latest revision changed that passage.
2. The passage is directly cited by an unresolved prior `NOT_READY` blocker.
3. The passage contains a protocol-breaking defect that would make final delivery unsafe.

If none of those gates applies, do not review, rewrite, restyle, reorder, summarize, expand, or replace that passage. Mark the concern as `OUT_OF_SCOPE`.
Every `NOT_READY` item must identify the exact passage or requirement that is unlocked and explain why the lock can be broken.

## Required Response Format

- The first line must be exactly `MAESTRO_STATUS: READY` or `MAESTRO_STATUS: NOT_READY`.
- Use READY only if the draft can be delivered as final text under the protocol.
- Use NOT_READY only for concrete blockers: factual/evidence failure, broken or unverifiable links, ABNT/protocol violation, hallucination/confabulation, material omission, or editorial defect that blocks delivery.
- After the status line, list only concrete blocking corrections, each tied to a specific passage or requirement.

## Previous Blocking Objections To Re-check

```markdown
{}
```

## Operator Request

{}

## Full Editorial Protocol

```markdown
{}
```

## Draft Under Review

```markdown
{}
```
{}
"#,
        sanitize_text(&request.session_name, 200),
        sanitize_text(draft_author_key, 80),
        previous_blocking_notes,
        request.prompt,
        request.protocol_text,
        draft,
        evidence_block
    )
}

#[cfg(test)]
pub(crate) fn build_review_objections_block(review_agents: &[EditorialAgentResult]) -> String {
    let mut review_notes = String::new();
    for agent in review_agents {
        if agent.role != "review" || agent.status == "READY" {
            continue;
        }
        if is_operational_agent_result(agent) {
            continue;
        }
        let artifact = read_text_file(Path::new(&agent.output_path)).unwrap_or_default();
        let stdout = extract_stdout_block(&artifact).unwrap_or_default().trim();
        if stdout.is_empty() {
            continue;
        }
        let useful_excerpt = stdout.chars().take(18_000).collect::<String>();
        review_notes.push_str(&format!(
            "\n### {} / {}\n\nStatus: `{}` (`{}`)\nArtifact: `{}`\n\n```markdown\n{}\n```\n",
            agent.name, agent.role, agent.status, agent.tone, agent.output_path, useful_excerpt
        ));
    }
    review_notes
}

pub(crate) fn build_revision_history_block(agents: &[EditorialAgentResult]) -> String {
    let mut history = String::new();
    for agent in agents {
        if agent.role != "review" && agent.role != "revision" {
            continue;
        }
        if is_operational_agent_result(agent) {
            continue;
        }
        let artifact = read_text_file(Path::new(&agent.output_path)).unwrap_or_default();
        let stdout = extract_stdout_block(&artifact).unwrap_or(artifact.as_str());
        let report = extract_tagged_block(stdout, "maestro_revision_report")
            .unwrap_or_else(|| {
                format!(
                    "No complete maestro_revision_report block was returned by {}. Treat this artifact as a contract failure, not as deliberative substance.",
                    agent.name
                )
            });
        let report = report.trim();
        if report.is_empty() {
            continue;
        }
        history.push_str(&format!(
            "\n### {} / {} / `{}`\n\nArtifact: `{}`\n\n```text\n{}\n```\n",
            agent.name,
            agent.role,
            agent.status,
            agent.output_path,
            report.chars().take(12_000).collect::<String>()
        ));
    }
    if history.trim().is_empty() {
        "No prior revision reports are recorded for this serial cycle.".to_string()
    } else {
        history
    }
}

pub(crate) fn is_operational_agent_result(agent: &EditorialAgentResult) -> bool {
    agent.tone == "error"
        || agent.tone == "blocked"
        || agent.status == "RUNNING"
        || agent.status == "AGENT_FAILED_NO_OUTPUT"
        || agent.status == "AGENT_FAILED_EMPTY"
        || agent.status == "EMPTY_DRAFT"
        || agent.status == "STOPPED_BY_USER"
        || agent.status == "CLI_NOT_FOUND"
        || agent.status == "COST_LIMIT_REACHED"
        || agent.status == "API_KEY_NOT_AVAILABLE"
        || agent.status == "REMOTE_SECRET_NOT_READABLE"
        || agent.status == "CODEX_CLI_NO_FINAL_OUTPUT"
        || agent.status == "CODEX_WINDOWS_SANDBOX_UPSTREAM"
        || agent.status == "GEMINI_CLI_NO_FINAL_OUTPUT"
        || agent.status == "GEMINI_RIPGREP_UNAVAILABLE"
        || agent.status == "GEMINI_WORKSPACE_VIOLATION"
        || agent.status.starts_with("EXEC_ERROR")
        || agent.status.starts_with("PROVIDER_")
}

pub(crate) fn build_serial_revision_prompt(
    request: &EditorialSessionRequest,
    run_id: &str,
    turn: usize,
    current_text: &str,
    current_author_key: &str,
    reviewer_key: &str,
    closing_turn: bool,
    previous_revision_history: &str,
    evidence_block: &str,
) -> String {
    format!(
        r#"# Maestro Editorial AI - Serial Review-Rewrite Turn

Run: `{run_id}`
Round turn: `{turn}`
Session: {}

## Language Contract

- Internal coordination, critique, changelog, and revision report MUST be written in en_US.
- The operator-facing article inside `<maestro_final_text>` MUST be written in Brazilian Portuguese (pt_BR).
- Keep protocol markers exactly as specified.
- The editorial protocol is authoritative input, not output. Read and obey it, but do not quote, summarize, restate, or reproduce protocol text in the artifact. Cite compact section IDs only, such as `§V.14` or `§11.7`.

## Role Contract

- Current version author/curator: `{}`.
- Current reviewer-reviser: `{}`.
- Closing redactor turn: `{}`.
- You are not allowed to revise a version you just produced.
- If you are the current version author and this is not the closing redactor turn, return `MAESTRO_STATUS: NOT_READY` and state `SELF_REVIEW_BLOCKED`.
- If you are the current version author during the closing redactor turn, audit only the completed peer circuit and leave custody `"unchanged"` unless another agent has produced the current version.
- You must act as reviewer and reviser in one turn: inspect the current text, apply only authorized corrections, and return the complete current article.
- A Maestro round is a full circular pass through all active AI agents. This call is one turn inside that round; do not call it a new round in your own report.

## Sovereign Approved-Content Lock

Approved content is locked by default.
You may alter a passage only when at least one hard gate applies:

1. A prior revision report or blocker explicitly cites that passage.
2. The passage contains a concrete, protocol-grounded defect that blocks safe final delivery.
3. A tiny adjacent edit is strictly necessary to keep grammar or continuity after an authorized correction.

If none of those gates applies, preserve the passage exactly. Do not restyle, shorten, reorder, simplify, expand, or replace it.
If a concern is optional, stylistic, vague, or outside scope, mark it as `OUT_OF_SCOPE` in the report and leave the text unchanged.

## Quality Preservation / Anti-Impoverishment Gate

Codex and Claude are the strongest long-form writers in this system. Gemini is second. DeepSeek and Grok are useful reviewers but must not flatten stronger prose.
Preserve the strongest existing formulation unless a concrete editorial-protocol defect requires a narrow change.
Do not reduce breadth, depth, articulation, nuance, reflexivity, or argumentative amplitude.
Any deletion, compression, simplification, or structural narrowing must be justified in the report with:

- the exact passage changed;
- the exact protocol requirement;
- why preserving the stronger formulation would be unsafe or incorrect.

If you are unsure, preserve the passage and report the concern instead of rewriting it.

## Required Output Contract

The answer MUST contain exactly these parts:

1. First line: `MAESTRO_STATUS: READY` or `MAESTRO_STATUS: NOT_READY`.
2. `<maestro_revision_report>` containing en_US JSON-like audit data:
   - `reviewer`
   - `current_author`
   - `status`
   - `changes`: list of changed passages, received line/passage reference, reason, protocol citation, and whether the change was required.
   - `out_of_scope`: concerns intentionally not changed.
   - `quality_preservation`: explicit statement that approved strong formulations were preserved; if not, justify each reduction.
   - `custody`: exactly `"revised"` when you changed the article, or exactly `"unchanged"` when you approve/criticize without changing custody.
3. Include `<maestro_final_text>` containing only the complete operator-facing article in pt_BR only when `custody` is `"revised"`.
4. If `custody` is `"unchanged"`, omit `<maestro_final_text>` entirely. Do not repeat the current article.

Anything outside those tags may be discarded by the app.
An incomplete tag, missing closing tag, reproduced protocol text, or truncated JSON/report is a contract violation and will not count as READY.

## Operator Request

{}

## Full Editorial Protocol

```markdown
{}
```

## Current Text Under Custody

```markdown
{}
```

## Prior Serial Revision Reports

{}
{}
"#,
        sanitize_text(&request.session_name, 200),
        sanitize_text(current_author_key, 80),
        sanitize_text(reviewer_key, 80),
        closing_turn,
        request.prompt,
        request.protocol_text,
        current_text,
        previous_revision_history,
        evidence_block
    )
}

#[cfg(test)]
pub(crate) fn build_revision_prompt(
    request: &EditorialSessionRequest,
    run_id: &str,
    round: usize,
    draft: &str,
    review_agents: &[EditorialAgentResult],
    evidence_block: &str,
) -> String {
    let review_notes = build_review_objections_block(review_agents);

    format!(
        r#"# Maestro Editorial AI - Internal Revision Request

Run: `{run_id}`
Review round: `{round}`
Session: {}

## Language Contract

- Internal deliberation is in en_US.
- The operator-facing revised text MUST be written in Brazilian Portuguese (pt_BR).
- Do not include internal comments, vote analysis, or process notes in the final stdout.

## Revision Boundary Contract

This is a new deliberative cycle inside the same case file. Preserve the append-only history.
Your task is to return a complete revised Markdown text, but edits are strictly limited:

- Treat every passage not cited by the concrete blockers below as locked approved content.
- Change only passages required to resolve concrete `NOT_READY` blocking objections listed below.
- Preserve approved paragraphs, structure, references, wording, and claims unless a listed blocker directly requires changing them.
- Do not "improve", restyle, reorganize, summarize, expand, or replace content that was not challenged by a concrete blocker.
- If a blocker requires a small adjacent edit for grammar or continuity, make only that adjacent edit and keep the surrounding approved content intact.
- If a reviewer gave a broad instruction such as "read everything again" or "rewrite the text", reduce it to the specific concrete blockers actually stated.
- If an objection is vague, optional, stylistic, or outside the blocking scope, do not rewrite the text for it.
- If no concrete blocking objection below authorizes a change, return the current draft unchanged.
- Do not invent links. If evidence is missing, preserve `[EVIDENCIA_PENDENTE]`.

Write the entire revised text only to stdout. Do not create local files.

## Operator Request

{}

## Full Editorial Protocol

```markdown
{}
```

## Current Draft

```markdown
{}
```

## Concrete Blocking Objections From Reviewers

{}
{}
"#,
        sanitize_text(&request.session_name, 200),
        request.prompt,
        request.protocol_text,
        draft,
        review_notes,
        evidence_block
    )
}

#[cfg(test)]
mod tests {
    use super::{
        build_draft_prompt, build_review_objections_block, build_review_prompt,
        build_revision_history_block, build_revision_prompt, build_serial_revision_prompt,
    };
    use crate::{app_paths::sessions_dir, EditorialAgentResult, EditorialSessionRequest};
    use std::path::PathBuf;

    fn test_request() -> EditorialSessionRequest {
        EditorialSessionRequest {
            run_id: "run-test".to_string(),
            session_name: "Sessao teste".to_string(),
            prompt: "Escreva um artigo.".to_string(),
            protocol_name: "protocolo.md".to_string(),
            protocol_text: "Regra editorial completa com mais de cem caracteres para simular o protocolo integral usado pelo Maestro em producao.".to_string(),
            protocol_hash: "hash".to_string(),
            initial_agent: Some("claude".to_string()),
            active_agents: Some(vec!["claude".to_string(), "codex".to_string()]),
            max_session_cost_usd: Some(1.0),
            max_session_minutes: None,
            attachments: None,
            links: None,
        }
    }

    #[test]
    fn draft_prompt_marks_redactor_as_petitioner_not_reviewer() {
        let prompt = build_draft_prompt(&test_request(), "run-test", "");

        assert!(prompt.contains("petitioner/drafter"));
        assert!(prompt.contains("never vote as reviewer of your own text"));
        assert!(prompt.contains("Brazilian Portuguese (pt_BR)"));
        assert!(prompt.contains("en_US"));
    }

    #[test]
    fn review_prompt_carries_tribunal_self_review_guard() {
        let prompt = build_review_prompt(
            &test_request(),
            "run-test",
            2,
            "rascunho",
            "claude",
            "Prior blocker.",
            "",
        );

        assert!(prompt.contains("Collegiate Review Contract"));
        assert!(prompt.contains("Text author/petitioner under review: `claude`"));
        assert!(prompt.contains("SELF_REVIEW_BLOCKED"));
        assert!(prompt.contains("Round 2 and later"));
        assert!(prompt.contains("Do NOT reopen or rewrite content that has already been accepted"));
        assert!(prompt.contains("Approved Content Lock"));
        assert!(prompt.contains("Every `NOT_READY` item must identify the exact passage"));
        assert!(prompt.contains("Prior blocker."));
    }

    #[test]
    fn revision_prompt_names_append_only_deliberative_cycle() {
        let prompt = build_revision_prompt(&test_request(), "run-test", 2, "rascunho", &[], "");

        assert!(prompt.contains("new deliberative cycle inside the same case file"));
        assert!(prompt.contains("Change only passages required"));
        assert!(prompt.contains("Preserve approved paragraphs"));
        assert!(prompt.contains("Treat every passage not cited by the concrete blockers below as locked approved content"));
        assert!(prompt.contains("return the current draft unchanged"));
    }

    #[test]
    fn serial_revision_prompt_separates_report_from_final_text_and_quality_gate() {
        let prompt = build_serial_revision_prompt(
            &test_request(),
            "run-test",
            3,
            "Texto atual",
            "codex",
            "deepseek",
            false,
            "No prior reports.",
            "",
        );

        assert!(prompt.contains("Serial Review-Rewrite Turn"));
        assert!(prompt.contains("<maestro_revision_report>"));
        assert!(prompt.contains("<maestro_final_text>"));
        assert!(prompt.contains("Quality Preservation / Anti-Impoverishment Gate"));
        assert!(prompt.contains("must not flatten stronger prose"));
        assert!(prompt.contains("Internal coordination, critique, changelog, and revision report MUST be written in en_US"));
    }

    #[test]
    fn review_objections_block_excludes_ready_votes() {
        let dir = sessions_dir().join(format!("maestro-review-objections-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let ready_path = dir.join("ready.md");
        let not_ready_path = dir.join("not-ready.md");
        std::fs::write(
            &ready_path,
            "# Ready\n\n## Stdout\n\n```text\nMAESTRO_STATUS: READY\nApproved.\n```\n\n## Stderr\n\n```text\n\n```\n",
        )
        .unwrap();
        std::fs::write(
            &not_ready_path,
            "# Not Ready\n\n## Stdout\n\n```text\nMAESTRO_STATUS: NOT_READY\nFix citation 3 only.\n```\n\n## Stderr\n\n```text\n\n```\n",
        )
        .unwrap();
        let agents = vec![
            test_agent("Claude", "READY", &ready_path),
            test_agent("Gemini", "NOT_READY", &not_ready_path),
        ];

        let block = build_review_objections_block(&agents);

        assert!(!block.contains("Approved."));
        assert!(block.contains("Fix citation 3 only."));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn revision_history_block_extracts_internal_report_only() {
        let dir = sessions_dir().join(format!("maestro-revision-history-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let artifact_path = dir.join("round-002-codex-revision.md");
        std::fs::write(
            &artifact_path,
            "# Codex\n\n## Stdout\n\n```text\nMAESTRO_STATUS: READY\n<maestro_revision_report>{\"changes\":[]}</maestro_revision_report>\n<maestro_final_text>Texto final</maestro_final_text>\n```\n",
        )
        .unwrap();
        let agents = vec![EditorialAgentResult {
            name: "Codex".to_string(),
            role: "review".to_string(),
            cli: "codex".to_string(),
            tone: "ok".to_string(),
            status: "READY".to_string(),
            duration_ms: 1,
            exit_code: Some(0),
            output_path: artifact_path.to_string_lossy().to_string(),
            usage_input_tokens: None,
            usage_output_tokens: None,
            cost_usd: None,
            cost_estimated: None,
            cache: None,
        }];

        let block = build_revision_history_block(&agents);

        assert!(block.contains("{\"changes\":[]}"));
        assert!(!block.contains("Texto final"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn review_objections_block_excludes_operational_failures() {
        let dir = sessions_dir().join(format!("maestro-review-operational-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let failed_path = dir.join("failed.md");
        let not_ready_path = dir.join("not-ready.md");
        std::fs::write(
            &failed_path,
            "# Failed\n\n## Stdout\n\n```text\n\n```\n\n## Stderr\n\n```text\n\n```\n",
        )
        .unwrap();
        std::fs::write(
            &not_ready_path,
            "# Not Ready\n\n## Stdout\n\n```text\nMAESTRO_STATUS: NOT_READY\nFix the cited paragraph.\n```\n\n## Stderr\n\n```text\n\n```\n",
        )
        .unwrap();
        let agents = vec![
            test_agent("Claude", "AGENT_FAILED_NO_OUTPUT", &failed_path),
            test_agent("Gemini", "NOT_READY", &not_ready_path),
        ];

        let block = build_review_objections_block(&agents);

        assert!(!block.contains("AGENT_FAILED_NO_OUTPUT"));
        assert!(!block.contains("No usable editorial review"));
        assert!(block.contains("Fix the cited paragraph."));
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn test_agent(name: &str, status: &str, path: &PathBuf) -> EditorialAgentResult {
        EditorialAgentResult {
            name: name.to_string(),
            role: "review".to_string(),
            cli: name.to_ascii_lowercase(),
            tone: if status == "READY" { "ok" } else { "warn" }.to_string(),
            status: status.to_string(),
            duration_ms: 1,
            exit_code: Some(0),
            output_path: path.to_string_lossy().to_string(),
            usage_input_tokens: None,
            usage_output_tokens: None,
            cost_usd: None,
            cost_estimated: None,
            cache: None,
        }
    }
}
