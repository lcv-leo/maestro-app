// Modulo: src-tauri/src/editorial_prompts.rs
// Descricao: Editorial agent specs (CLI args + spec table) and prompt builders
// (draft/review/revision) extracted from lib.rs in v0.3.25 per
// `docs/code-split-plan.md` migration step 5.
//
// What's here (10 functions):
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
    extract_stdout_block, read_text_file, sanitize_text, EditorialAgentResult, EditorialAgentSpec,
    EditorialSessionRequest,
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
        "Leia integralmente o bloco <stdin> fornecido pelo Maestro e responda conforme as instrucoes.".to_string(),
    ]
}

pub(crate) fn gemini_args() -> Vec<String> {
    vec![
        "--prompt".to_string(),
        "Leia o stdin integralmente e responda conforme as instrucoes do Maestro.".to_string(),
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
        r#"# Maestro Editorial AI - Geracao Real

Run: `{run_id}`
Sessao: {}

Voce e o agente redator escolhido para abrir a sessao editorial. Leia integralmente o protocolo abaixo antes de escrever.
Gere um rascunho em Markdown puro para a solicitacao do operador.
Nao crie arquivos locais. Escreva a resposta inteira somente na saida padrao da CLI.
Nao invente links. Se faltar evidencia, marque explicitamente `[EVIDENCIA_PENDENTE]`.

## Solicitacao do operador

{}

## Protocolo editorial integral

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

pub(crate) fn build_review_prompt(
    request: &EditorialSessionRequest,
    run_id: &str,
    draft: &str,
    evidence_block: &str,
) -> String {
    format!(
        r#"# Maestro Editorial AI - Revisao Real

Run: `{run_id}`
Sessao: {}

Leia integralmente o protocolo editorial e revise o rascunho abaixo.
Responda em Markdown.

Obrigatorio:
- A primeira linha deve ser exatamente `MAESTRO_STATUS: READY` ou `MAESTRO_STATUS: NOT_READY`.
- Use READY somente se o rascunho pode ser entregue como texto final conforme o protocolo.
- Use NOT_READY se houver falhas, links a verificar, violacao ABNT, falta de evidencia, confabulacao, ou problema editorial.
- Liste correcoes concretas.

## Solicitacao do operador

{}

## Protocolo editorial integral

```markdown
{}
```

## Rascunho a revisar

```markdown
{}
```
{}
"#,
        sanitize_text(&request.session_name, 200),
        request.prompt,
        request.protocol_text,
        draft,
        evidence_block
    )
}

pub(crate) fn build_revision_prompt(
    request: &EditorialSessionRequest,
    run_id: &str,
    round: usize,
    draft: &str,
    review_agents: &[EditorialAgentResult],
    evidence_block: &str,
) -> String {
    let mut review_notes = String::new();
    for agent in review_agents {
        let artifact = read_text_file(Path::new(&agent.output_path)).unwrap_or_default();
        let stdout = extract_stdout_block(&artifact).unwrap_or_default().trim();
        let useful_excerpt = if stdout.is_empty() {
            format!(
                "Sem parecer editorial utilizavel nesta tentativa. Status operacional: {} / {}.",
                agent.status, agent.tone
            )
        } else {
            stdout.chars().take(18_000).collect::<String>()
        };
        review_notes.push_str(&format!(
            "\n### {} / {}\n\nStatus: `{}` (`{}`)\nArtifact: `{}`\n\n```markdown\n{}\n```\n",
            agent.name, agent.role, agent.status, agent.tone, agent.output_path, useful_excerpt
        ));
    }

    format!(
        r#"# Maestro Editorial AI - Revisao de Rascunho

Run: `{run_id}`
Rodada de revisao: `{round}`
Sessao: {}

Leia integralmente o protocolo editorial, o rascunho atual e as manifestacoes dos peers.
Sua tarefa e produzir uma nova versao completa do texto em Markdown puro, incorporando todas as correcoes concretas.
Nao entregue comentarios sobre o processo. Entregue apenas o texto revisado.
Nao crie arquivos locais. Escreva a resposta inteira somente na saida padrao da CLI.
Nao invente links. Se faltar evidencia, preserve marcador `[EVIDENCIA_PENDENTE]`.

## Solicitacao do operador

{}

## Protocolo editorial integral

```markdown
{}
```

## Rascunho atual

```markdown
{}
```

## Manifestacoes dos peers

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
