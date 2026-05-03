// Modulo: src-tauri/src/session_minutes.rs
// Descricao: `ata-da-sessao.md` (session minutes) text generators extracted
// from lib.rs in v0.3.37 per `docs/code-split-plan.md` migration step 5.
//
// What's here (2 functions):
//   - `build_session_minutes` — builds the markdown body of
//     `ata-da-sessao.md` with header (run id, session name, protocol,
//     consensus flag, final-text path, peers, caps), per-agent bullets,
//     and the closing "Decisao" section. Branches into
//     `build_blocked_minutes_decision` when consensus did not converge.
//   - `build_blocked_minutes_decision` — explains why the session did not
//     reach unanimity: counts READY reviews / operational failures /
//     editorial divergences, and lists the most recent 8 of each (reversed
//     so the latest rounds appear first).
//
// What stays in lib.rs (consumed via `pub(crate)` imports):
//   - `EditorialSessionRequest` (pub(crate) since v0.3.25 via
//     editorial_prompts), `EditorialAgentResult` (pub(crate)).
//   - `sanitize_text`, `sanitize_short` (re-exported via lib.rs since v0.3.34).
//   - `all_agent_keys` from `session_controls` (already pub(crate)).
//
// v0.3.37 is a pure move: every signature, format string, and marker
// ("Texto final liberado por unanimidade...", "A regra permanece:
// divergencia editorial exige novas rodadas...", etc.) is identical to
// the v0.3.36 lib.rs source (commit e199c1b).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::session_controls::all_agent_keys;
use crate::{sanitize_short, sanitize_text, EditorialAgentResult, EditorialSessionRequest};

pub(crate) fn build_session_minutes(
    request: &EditorialSessionRequest,
    run_id: &str,
    agents: &[EditorialAgentResult],
    consensus_ready: bool,
    final_path: Option<&PathBuf>,
) -> String {
    let mut text = format!(
        "# Ata da Sessao Maestro\n\n- Run: `{run_id}`\n- Sessao: {}\n- Protocolo: `{}`\n- Hash do protocolo: `{}`\n- Consenso unanime: `{}`\n- Texto final: `{}`\n- Peers ativos: `{}`\n- Limite de custo: `{}`\n- Limite de tempo: `{}`\n\n## Solicitacao\n\n{}\n",
        sanitize_text(&request.session_name, 200),
        sanitize_text(&request.protocol_name, 200),
        sanitize_short(&request.protocol_hash, 80),
        consensus_ready,
        final_path
            .map(|path| path.to_string_lossy().to_string())
            .unwrap_or_else(|| "bloqueado".to_string()),
        request
            .active_agents
            .clone()
            .unwrap_or_else(all_agent_keys)
            .join(", "),
        request
            .max_session_cost_usd
            .map(|value| format!("US$ {value:.4}"))
            .unwrap_or_else(|| "ignorado".to_string()),
        request
            .max_session_minutes
            .map(|value| format!("{value} min"))
            .unwrap_or_else(|| "ignorado".to_string()),
        request.prompt
    );

    let mut agents_by_round = BTreeMap::<usize, Vec<&EditorialAgentResult>>::new();
    for agent in agents {
        agents_by_round
            .entry(agent_round_from_output_path(agent))
            .or_default()
            .push(agent);
    }

    for (round, round_agents) in agents_by_round {
        if round == 0 {
            text.push_str("\n## Rodada sem numero\n\n");
        } else {
            text.push_str(&format!("\n## Rodada {round:03}\n\n"));
        }
        for agent in round_agents {
            text.push_str(&format!(
                "- **{} / {}**: `{}` (`{}`), {} ms, artifact: `{}`\n",
                agent.name,
                agent.role,
                agent.status,
                agent.tone,
                agent.duration_ms,
                agent.output_path
            ));
        }
    }

    if !consensus_ready {
        text.push_str("\n## Decisao\n\n");
        text.push_str(&build_blocked_minutes_decision(agents));
    } else {
        text.push_str("\n## Decisao\n\nTexto final liberado por unanimidade dos agentes.\n");
    }

    text
}

fn agent_round_from_output_path(agent: &EditorialAgentResult) -> usize {
    Path::new(&agent.output_path)
        .file_name()
        .and_then(|name| name.to_str())
        .and_then(round_from_agent_artifact_name)
        .unwrap_or(0)
}

fn round_from_agent_artifact_name(name: &str) -> Option<usize> {
    let rest = name.strip_prefix("round-")?;
    let (round, _) = rest.split_once('-')?;
    round.parse::<usize>().ok()
}

pub(crate) fn build_blocked_minutes_decision(agents: &[EditorialAgentResult]) -> String {
    let review_agents = agents
        .iter()
        .filter(|agent| agent.role == "review")
        .collect::<Vec<_>>();
    let ready_reviews = review_agents
        .iter()
        .filter(|agent| agent.status == "READY")
        .count();
    let operational_failures = agents
        .iter()
        .filter(|agent| {
            agent.tone == "error"
                || agent.tone == "blocked"
                || agent.status == "RUNNING"
                || agent.status == "AGENT_FAILED_NO_OUTPUT"
                || agent.status == "AGENT_FAILED_EMPTY"
                || agent.status == "EMPTY_DRAFT"
                || agent.status.starts_with("EXEC_ERROR")
        })
        .collect::<Vec<_>>();
    let editorial_divergences = review_agents
        .iter()
        .filter(|agent| agent.status != "READY" && agent.tone != "error" && agent.tone != "blocked")
        .collect::<Vec<_>>();

    let mut text = format!(
        "Texto final indisponivel nesta chamada.\n\n- Revisoes READY registradas: {ready_reviews}/{}.\n- Falhas operacionais detectadas: {}.\n- Divergencias editoriais ainda abertas: {}.\n",
        review_agents.len(),
        operational_failures.len(),
        editorial_divergences.len()
    );

    if !operational_failures.is_empty() {
        text.push_str("\n### Falhas operacionais\n\n");
        for agent in operational_failures.iter().rev().take(8) {
            text.push_str(&format!(
                "- **{} / {}**: `{}` (`{}`), exit code `{}`, artifact: `{}`\n",
                agent.name,
                agent.role,
                agent.status,
                agent.tone,
                agent
                    .exit_code
                    .map(|code| code.to_string())
                    .unwrap_or_else(|| "unknown".to_string()),
                agent.output_path
            ));
        }
    }

    if !editorial_divergences.is_empty() {
        text.push_str("\n### Divergencias editoriais\n\n");
        for agent in editorial_divergences.iter().rev().take(8) {
            text.push_str(&format!(
                "- **{} / {}**: `{}` (`{}`), artifact: `{}`\n",
                agent.name, agent.role, agent.status, agent.tone, agent.output_path
            ));
        }
    }

    text.push_str(
        "\nA regra permanece: divergencia editorial exige novas rodadas ate unanimidade; falha operacional exige retry ou intervencao do operador antes de qualquer entrega final.\n",
    );
    text
}

#[cfg(test)]
mod tests {
    use super::build_session_minutes;
    use crate::{EditorialAgentResult, EditorialSessionRequest};

    fn test_request() -> EditorialSessionRequest {
        EditorialSessionRequest {
            run_id: "run-test".to_string(),
            session_name: "Sessao".to_string(),
            prompt: "Escreva o texto.".to_string(),
            protocol_name: "Protocolo".to_string(),
            protocol_text: "Regras".to_string(),
            protocol_hash: "hash".to_string(),
            initial_agent: Some("claude".to_string()),
            active_agents: Some(vec!["claude".to_string(), "gemini".to_string()]),
            max_session_cost_usd: None,
            max_session_minutes: None,
            attachments: None,
            links: None,
        }
    }

    fn test_agent(name: &str, role: &str, status: &str, output_path: &str) -> EditorialAgentResult {
        EditorialAgentResult {
            name: name.to_string(),
            role: role.to_string(),
            cli: name.to_ascii_lowercase(),
            tone: "ok".to_string(),
            status: status.to_string(),
            duration_ms: 100,
            exit_code: Some(0),
            output_path: output_path.to_string(),
            usage_input_tokens: None,
            usage_output_tokens: None,
            cost_usd: None,
            cost_estimated: None,
        }
    }

    #[test]
    fn build_session_minutes_groups_agents_by_real_artifact_round() {
        let agents = vec![
            test_agent(
                "Claude",
                "draft",
                "READY",
                "agent-runs/round-001-claude-draft.md",
            ),
            test_agent(
                "Gemini",
                "review",
                "NOT_READY",
                "agent-runs/round-002-gemini-review.md",
            ),
        ];

        let minutes = build_session_minutes(&test_request(), "run-test", &agents, false, None);

        let round_001 = minutes.find("## Rodada 001").unwrap();
        let claude = minutes.find("Claude / draft").unwrap();
        let round_002 = minutes.find("## Rodada 002").unwrap();
        let gemini = minutes.find("Gemini / review").unwrap();
        assert!(round_001 < claude);
        assert!(claude < round_002);
        assert!(round_002 < gemini);
        assert_eq!(minutes.matches("## Rodada 001").count(), 1);
    }

    #[test]
    fn build_session_minutes_keeps_unparseable_artifact_rounds_visible() {
        let agents = vec![test_agent(
            "Claude",
            "review",
            "READY",
            "agent-runs/claude-review.md",
        )];

        let minutes = build_session_minutes(&test_request(), "run-test", &agents, true, None);

        assert!(minutes.contains("## Rodada sem numero"));
        assert!(minutes.contains("Claude / review"));
    }
}
