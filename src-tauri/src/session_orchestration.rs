// Modulo: src-tauri/src/session_orchestration.rs
// Descricao: Editorial session orchestration loop extracted from lib.rs in
// v0.5.8 per `docs/code-split-plan.md`. Holds the two big helpers
// `run_editorial_session_inner` (thin wrapper) and `run_editorial_session_core`
// (the ~920-line round loop). Pure refactor from lib.rs v0.5.7
// (commit 7a6a451): function bodies preserved verbatim.
//
// What stays here:
//   - Cancellation token propagation: between-rounds checkpoint emits
//     `session.user.stop_completed` and returns status `STOPPED_BY_USER`.
//   - `FinalizeRunningArtifactsGuard` so a panicking Drop normalizes the
//     RUNNING agent artifacts left behind.
//   - Contract resolution applied per the v0.5.2 / v0.3.42 invariant:
//     request is source of truth for caps and active_agents; saved contract
//     is reference only.
//   - Cost preflight loop: `provider_cost_rates_from_config` per API agent;
//     missing rates short-circuit with a structured-error result.
//   - The 5-phase loop body (initial draft + 3 review rounds + revision /
//     finalize). All NDJSON shapes preserved.
//
// What stays in lib.rs after this batch:
//   - `pub(crate) use crate::editorial_io::{...}` shim used by sibling
//     modules (provider_runners, provider_deepseek, command_spawn).
//   - `SessionArtifact` struct (used by session_artifacts and session_resume).
//   - `pub fn run()` Tauri 2 entry point.

use chrono::Utc;
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use crate::app_paths::{
    checked_data_child_path, human_log_path_for, sanitize_path_segment, sessions_dir,
};
use crate::editorial_agent_runners::run_editorial_agent_for_spec;
use crate::editorial_helpers::{
    filter_existing_agents_to_active_set, resolve_effective_active_agents,
    FinalizeRunningArtifactsGuard,
};
use crate::editorial_inputs::{
    build_active_agents_resolved_log_context, resolve_time_budget_anchor,
};
use crate::editorial_io::{
    editorial_session_result, extract_stdout_block, extract_tagged_block, read_text_file,
    strip_leading_maestro_status, write_text_file, SessionResultContext,
};
#[cfg(test)]
use crate::editorial_prompts::is_operational_agent_result;
use crate::editorial_prompts::{
    build_draft_prompt, build_revision_history_block, build_serial_revision_prompt,
};
use crate::logging::{write_log_record, LogEventInput, LogSession};
use crate::provider_config::{
    api_provider_for_agent, provider_cost_rates_from_config, should_run_agent_via_api,
};
use crate::session_artifacts::parse_agent_artifact_name;
use crate::session_controls::{
    api_role_max_tokens, effective_draft_lead, estimate_provider_cost_from_input_chars,
    provider_cost_guard_for, sanitize_optional_positive_f64, sanitize_optional_positive_u64,
    selected_editorial_agent_specs,
};
use crate::session_evidence::process_session_evidence;
use crate::session_minutes::build_session_minutes;
use crate::session_persistence::{
    append_agent_cost_to_ledger, load_cost_ledger, load_session_contract, write_session_contract,
};
use crate::session_resume::{parse_created_at, remaining_session_duration, session_time_exhausted};
use crate::tauri_commands::read_ai_provider_config;
#[cfg(test)]
use crate::EditorialAgentResult;
use crate::{
    api_input_estimate_chars, sanitize_text, AiProviderConfig, EditorialSessionRequest,
    EditorialSessionResult, ResumeSessionState, SessionContract,
};

pub(crate) fn run_editorial_session_inner(
    request: &EditorialSessionRequest,
    log_session: &LogSession,
    cancel_token: &tokio_util::sync::CancellationToken,
) -> Result<EditorialSessionResult, String> {
    run_editorial_session_core(request, log_session, None, cancel_token)
}

pub(crate) fn run_editorial_session_core(
    request: &EditorialSessionRequest,
    log_session: &LogSession,
    resume_state: Option<ResumeSessionState>,
    cancel_token: &tokio_util::sync::CancellationToken,
) -> Result<EditorialSessionResult, String> {
    let run_id = sanitize_path_segment(&request.run_id, 120);
    if run_id.is_empty() {
        return Err("run_id vazio".to_string());
    }

    let prompt = request.prompt.trim();
    if prompt.is_empty() {
        return Err("prompt editorial vazio".to_string());
    }
    if request.protocol_text.trim().len() < 100 {
        return Err("protocolo editorial integral nao foi carregado".to_string());
    }
    let session_dir = checked_data_child_path(&sessions_dir().join(&run_id))?;
    let agent_dir = checked_data_child_path(&session_dir.join("agent-runs"))?;
    fs::create_dir_all(&agent_dir)
        .map_err(|error| format!("failed to create session dir: {error}"))?;
    let _finalize_guard = FinalizeRunningArtifactsGuard::new(agent_dir.clone());
    let cost_scope_id = format!(
        "{run_id}::cost-scope-{}-{}",
        log_session.id,
        Utc::now().timestamp_nanos_opt().unwrap_or_default()
    );

    let saved_contract = load_session_contract(&session_dir);
    let (active_agent_keys, active_agents_source) = resolve_effective_active_agents(
        request.active_agents.as_ref(),
        saved_contract
            .as_ref()
            .map(|contract| &contract.active_agents),
    )?;
    let (draft_lead_key, invalid_initial_agent) =
        effective_draft_lead(request.initial_agent.as_deref(), &active_agent_keys);
    let draft_lead_name = selected_editorial_agent_specs(draft_lead_key, &active_agent_keys)
        .first()
        .map(|spec| spec.name)
        .unwrap_or("Claude");
    let mut log_context = build_active_agents_resolved_log_context(
        &run_id,
        request.active_agents.as_ref(),
        saved_contract.as_ref(),
        &active_agent_keys,
        active_agents_source,
        draft_lead_key,
        invalid_initial_agent.as_deref(),
        request.max_session_cost_usd,
        request.max_session_minutes,
    );
    if let Some(context) = log_context.as_object_mut() {
        context.insert("cost_scope_id".to_string(), json!(cost_scope_id.clone()));
    }
    let _ = write_log_record(
        log_session,
        LogEventInput {
            level: "info".to_string(),
            category: "session.editorial.active_agents_resolved".to_string(),
            message: "effective active_agents and caps resolved before spawning".to_string(),
            context: Some(log_context),
        },
    );
    // B20 backend completion (v0.3.42): the operator's request value is
    // authoritative for cost/time caps — when frontend sends None (operator
    // left the form blank intending "no cap"), the saved contract's prior
    // cap MUST NOT be silently re-applied. Frontend B20 (v0.3.32) stopped
    // pre-populating the form, but the backend still fell back to
    // saved_contract via `request.max_session_cost_usd.or_else(...)`,
    // defeating the operator's explicit unlimited-budget request on resume.
    // Per the 2026-05-02 operator directive ("cada nova sessão, mesmo que
    // seja sessão retomada, deve ser livre para que o usuário defina novos
    // valores ou não"), the request alone is the source of truth.
    let max_session_cost_usd = sanitize_optional_positive_f64(request.max_session_cost_usd);
    let max_session_minutes = sanitize_optional_positive_u64(request.max_session_minutes);
    let created_at = saved_contract
        .as_ref()
        .map(|contract| parse_created_at(&contract.created_at))
        .unwrap_or_else(Utc::now);
    let time_budget_anchor =
        resolve_time_budget_anchor(created_at, resume_state.is_some(), Utc::now());
    let is_resume = resume_state.is_some();
    let original_initial_agent = saved_contract
        .as_ref()
        .and_then(|contract| contract.original_initial_agent.clone())
        .or_else(|| {
            saved_contract
                .as_ref()
                .map(|contract| contract.initial_agent.clone())
                .filter(|value| !value.trim().is_empty())
        })
        .unwrap_or_else(|| draft_lead_key.to_string());
    let ai_provider_config =
        read_ai_provider_config().unwrap_or_else(|_| AiProviderConfig::default());
    let mut cost_ledger = load_cost_ledger(&session_dir, &run_id, &cost_scope_id);
    let api_agent_keys = active_agent_keys
        .iter()
        .filter(|key| should_run_agent_via_api(key, &ai_provider_config))
        .cloned()
        .collect::<BTreeSet<_>>();
    if !api_agent_keys.is_empty() && max_session_cost_usd.is_none() {
        let _ = write_log_record(
            log_session,
            LogEventInput {
                level: "warn".to_string(),
                category: "session.cost.limit_required".to_string(),
                message: "API usage requires an explicit session cost limit before paid providers are called"
                    .to_string(),
                context: Some(json!({
                    "run_id": &run_id,
                    "api_agents": api_agent_keys.iter().cloned().collect::<Vec<_>>(),
                    "active_agents": active_agent_keys.clone(),
                    "policy": "paid_api_calls_require_operator_defined_max_session_cost_usd"
                })),
            },
        );
        let prompt_path = session_dir.join("prompt.md");
        let protocol_path = session_dir.join("protocolo.md");
        let _ = write_text_file(
            &prompt_path,
            &format!(
                "# Prompt da Sessao\n\nSessao: {}\nRun: `{}`\nAgente redator inicial original: `{}`\nAgente que assumiu esta chamada: `{}`\n\n{}",
                sanitize_text(&request.session_name, 200),
                run_id,
                original_initial_agent,
                draft_lead_key,
                prompt
            ),
        );
        let _ = write_text_file(&protocol_path, &request.protocol_text);
        let minutes_path = session_dir.join("ata-da-sessao.md");
        write_text_file(
            &minutes_path,
            &build_session_minutes(request, &run_id, &[], false, None),
        )?;
        return Ok(EditorialSessionResult {
            run_id,
            session_dir: session_dir.to_string_lossy().to_string(),
            final_markdown_path: None,
            session_minutes_path: minutes_path.to_string_lossy().to_string(),
            prompt_path: prompt_path.to_string_lossy().to_string(),
            protocol_path: protocol_path.to_string_lossy().to_string(),
            draft_path: None,
            agents: Vec::new(),
            consensus_ready: false,
            status: "PAUSED_COST_LIMIT_REQUIRED".to_string(),
            active_agents: active_agent_keys,
            max_session_cost_usd,
            max_session_minutes,
            observed_cost_usd: Some(cost_ledger.total_observed_cost_usd),
            links_path: None,
            attachments_manifest_path: None,
            human_log_path: Some(
                human_log_path_for(&log_session.path)
                    .to_string_lossy()
                    .to_string(),
            ),
        });
    }
    let mut provider_cost_rates = BTreeMap::new();
    for agent_key in &api_agent_keys {
        match provider_cost_rates_from_config(agent_key, &ai_provider_config) {
            Ok(rates) => {
                provider_cost_rates.insert(agent_key.clone(), rates);
            }
            Err(error) => {
                let _ = write_log_record(
                    log_session,
                    LogEventInput {
                        level: "error".to_string(),
                        category: "session.cost.config_missing".to_string(),
                        message: "API usage requires UI provider tariff configuration".to_string(),
                        context: Some(json!({
                            "run_id": &run_id,
                            "error": sanitize_text(&error, 500),
                            "agent": agent_key,
                            "provider": api_provider_for_agent(agent_key).unwrap_or("unknown"),
                            "active_agents": active_agent_keys.clone()
                        })),
                    },
                );
                let prompt_path = session_dir.join("prompt.md");
                let protocol_path = session_dir.join("protocolo.md");
                let _ = write_text_file(
                    &prompt_path,
                    &format!(
                            "# Prompt da Sessao\n\nSessao: {}\nRun: `{}`\nAgente redator inicial original: `{}`\nAgente que assumiu esta chamada: `{}`\n\n{}",
                            sanitize_text(&request.session_name, 200),
                            run_id,
                            original_initial_agent,
                            draft_lead_key,
                            prompt
                        ),
                );
                let _ = write_text_file(&protocol_path, &request.protocol_text);
                let minutes_path = session_dir.join("ata-da-sessao.md");
                write_text_file(
                    &minutes_path,
                    &build_session_minutes(request, &run_id, &[], false, None),
                )?;
                return Ok(EditorialSessionResult {
                    run_id,
                    session_dir: session_dir.to_string_lossy().to_string(),
                    final_markdown_path: None,
                    session_minutes_path: minutes_path.to_string_lossy().to_string(),
                    prompt_path: session_dir.join("prompt.md").to_string_lossy().to_string(),
                    protocol_path: session_dir
                        .join("protocolo.md")
                        .to_string_lossy()
                        .to_string(),
                    draft_path: None,
                    agents: Vec::new(),
                    consensus_ready: false,
                    status: "PAUSED_COST_RATES_MISSING".to_string(),
                    active_agents: active_agent_keys,
                    max_session_cost_usd,
                    max_session_minutes,
                    observed_cost_usd: Some(cost_ledger.total_observed_cost_usd),
                    links_path: None,
                    attachments_manifest_path: None,
                    human_log_path: Some(
                        human_log_path_for(&log_session.path)
                            .to_string_lossy()
                            .to_string(),
                    ),
                });
            }
        }
    }
    let evidence = process_session_evidence(
        &session_dir,
        request.links.as_ref(),
        request.attachments.as_ref(),
        saved_contract.as_ref(),
    )?;
    let contract = SessionContract {
        schema_version: 1,
        run_id: run_id.clone(),
        session_name: sanitize_text(&request.session_name, 200),
        created_at: created_at.to_rfc3339(),
        active_agents: active_agent_keys.clone(),
        initial_agent: original_initial_agent.clone(),
        original_initial_agent: Some(original_initial_agent.clone()),
        resume_lead: is_resume.then(|| draft_lead_key.to_string()),
        cycle_lead: Some(draft_lead_key.to_string()),
        cycle_started_at: Some(Utc::now().to_rfc3339()),
        max_session_cost_usd,
        max_session_minutes,
        links: evidence.links.clone(),
        attachments: evidence.attachments.clone(),
    };
    write_session_contract(&session_dir, &contract)?;

    let prompt_path = session_dir.join("prompt.md");
    let protocol_path = session_dir.join("protocolo.md");
    write_text_file(
        &prompt_path,
        &format!(
            "# Prompt da Sessao\n\nSessao: {}\nRun: `{}`\nAgente redator inicial original: `{}`\nAgente que assumiu esta chamada: `{}`\n\n{}",
            sanitize_text(&request.session_name, 200),
            run_id,
            original_initial_agent,
            draft_lead_key,
            prompt
        ),
    )?;
    write_text_file(&protocol_path, &request.protocol_text)?;
    let human_log_path = human_log_path_for(&log_session.path);

    let mut agents = Vec::new();
    let mut current_draft = String::new();
    let mut current_draft_path: Option<PathBuf> = None;
    let mut current_draft_author_key: Option<String> = None;
    let mut round = 1usize;
    let mut consecutive_reviewer_outage_rounds: u32 = 0;
    let mut stable_serial_approval_agents = BTreeSet::<String>::new();
    let mut serial_turns = 0usize;
    let mut round_turn_index = 0usize;
    let mut valid_round_agents = BTreeSet::<String>::new();
    let mut round_had_substantive_change = false;
    let mut round_had_editorial_divergence = false;
    const ALL_ERROR_ESCALATION_THRESHOLD: u32 = 3;

    if let Some(invalid_initial_agent) = invalid_initial_agent {
        let _ = write_log_record(
            log_session,
            LogEventInput {
                level: "warn".to_string(),
                category: "session.draft_lead.invalid".to_string(),
                message: "unknown initial editorial agent requested; falling back to Claude"
                    .to_string(),
                context: Some(json!({
                    "run_id": &run_id,
                    "requested_initial_agent": invalid_initial_agent,
                    "fallback_initial_agent": draft_lead_key
                })),
            },
        );
    }

    let _ = write_log_record(
        log_session,
        LogEventInput {
            level: "info".to_string(),
            category: "session.draft_lead.selected".to_string(),
            message: "editorial draft lead selected for initial draft and revision fallback order"
                .to_string(),
            context: Some(json!({
                "run_id": &run_id,
                "original_initial_agent": original_initial_agent,
                "cycle_lead": draft_lead_key,
                "cycle_lead_name": draft_lead_name,
                "resume_mode": is_resume,
                "active_agents": active_agent_keys.clone(),
                "agent_order": selected_editorial_agent_specs(draft_lead_key, &active_agent_keys)
                    .iter()
                    .map(|spec| spec.key)
                    .collect::<Vec<_>>()
            })),
        },
    );

    if let Some(state) = resume_state {
        agents = filter_existing_agents_to_active_set(state.existing_agents, &active_agent_keys);
        current_draft = state.current_draft;
        current_draft_path = state.current_draft_path;
        current_draft_author_key =
            current_draft_author_from_path(&agent_dir, current_draft_path.as_ref());
        round = state.next_review_round.max(1);
        let _ = write_log_record(
            log_session,
            LogEventInput {
                level: "info".to_string(),
                category: "session.resume.loaded".to_string(),
                message: "saved editorial session state loaded for continuation".to_string(),
                context: Some(json!({
                    "run_id": &run_id,
                    "next_review_round": round,
                    "current_draft_chars": current_draft.chars().count(),
                    "current_draft_path": current_draft_path.as_ref().map(|path| path.to_string_lossy().to_string()),
                    "current_draft_author_key": current_draft_author_key.clone(),
                    "existing_agent_artifacts": agents.len()
                })),
            },
        );
    }

    if current_draft.trim().is_empty() {
        let draft_specs = selected_editorial_agent_specs(draft_lead_key, &active_agent_keys);

        for spec in draft_specs {
            if session_time_exhausted(time_budget_anchor, max_session_minutes) {
                let minutes_path = session_dir.join("ata-da-sessao.md");
                write_text_file(
                    &minutes_path,
                    &build_session_minutes(request, &run_id, &agents, false, None),
                )?;
                let context = SessionResultContext {
                    run_id: &run_id,
                    session_dir: &session_dir,
                    prompt_path: &prompt_path,
                    protocol_path: &protocol_path,
                    active_agents: &active_agent_keys,
                    max_session_cost_usd,
                    max_session_minutes,
                    observed_cost_usd: cost_ledger.total_observed_cost_usd,
                    links_path: evidence.links_path.as_ref(),
                    attachments_manifest_path: evidence.attachments_manifest_path.as_ref(),
                    human_log_path: &human_log_path,
                };
                return Ok(editorial_session_result(
                    &context,
                    None,
                    &minutes_path,
                    current_draft_path,
                    agents,
                    false,
                    "TIME_LIMIT_REACHED",
                ));
            }
            let output_path = agent_attempt_output_path(&agent_dir, 1, spec.key, "draft");
            let timeout = remaining_session_duration(time_budget_anchor, max_session_minutes);
            let use_api_agent = api_agent_keys.contains(spec.key);
            let cost_guard = if use_api_agent {
                provider_cost_guard_for(
                    max_session_cost_usd,
                    provider_cost_rates.get(spec.key).copied(),
                    &cost_ledger,
                )
            } else {
                None
            };
            let draft_run = run_editorial_agent_for_spec(
                log_session,
                &run_id,
                spec,
                "draft",
                build_draft_prompt(request, &run_id, &evidence.block),
                &evidence.attachments,
                &output_path,
                timeout,
                &ai_provider_config,
                cost_guard,
                use_api_agent,
                cancel_token,
            );
            agents.push(draft_run.clone());
            append_agent_cost_to_ledger(
                &session_dir,
                &mut cost_ledger,
                &cost_scope_id,
                &draft_run,
            )?;
            if draft_run.status == "COST_LIMIT_REACHED" {
                let minutes_path = session_dir.join("ata-da-sessao.md");
                write_text_file(
                    &minutes_path,
                    &build_session_minutes(request, &run_id, &agents, false, None),
                )?;
                let context = SessionResultContext {
                    run_id: &run_id,
                    session_dir: &session_dir,
                    prompt_path: &prompt_path,
                    protocol_path: &protocol_path,
                    active_agents: &active_agent_keys,
                    max_session_cost_usd,
                    max_session_minutes,
                    observed_cost_usd: cost_ledger.total_observed_cost_usd,
                    links_path: evidence.links_path.as_ref(),
                    attachments_manifest_path: evidence.attachments_manifest_path.as_ref(),
                    human_log_path: &human_log_path,
                };
                return Ok(editorial_session_result(
                    &context,
                    None,
                    &minutes_path,
                    current_draft_path,
                    agents,
                    false,
                    "COST_LIMIT_REACHED",
                ));
            }
            let draft_artifact = read_text_file(&output_path).unwrap_or_default();
            let draft_text =
                extract_stdout_block(&draft_artifact).unwrap_or(draft_artifact.as_str());
            if draft_run.tone != "error"
                && draft_run.tone != "blocked"
                && !draft_text.trim().is_empty()
            {
                current_draft = draft_text.trim().to_string();
                current_draft_path = Some(output_path);
                current_draft_author_key = Some(spec.key.to_string());
                break;
            }

            let _ = write_log_record(
                log_session,
                LogEventInput {
                    level: "warn".to_string(),
                    category: "session.draft.retry".to_string(),
                    message: "draft agent did not produce usable text; trying next available agent"
                        .to_string(),
                    context: Some(json!({
                        "run_id": &run_id,
                        "agent": spec.name,
                        "status": draft_run.status,
                        "tone": draft_run.tone,
                        "next_policy": "continue_with_next_agent_without_final_delivery"
                    })),
                },
            );
        }
    }

    if current_draft.trim().is_empty() {
        let minutes_path = session_dir.join("ata-da-sessao.md");
        write_text_file(
            &minutes_path,
            &build_session_minutes(request, &run_id, &agents, false, None),
        )?;

        let context = SessionResultContext {
            run_id: &run_id,
            session_dir: &session_dir,
            prompt_path: &prompt_path,
            protocol_path: &protocol_path,
            active_agents: &active_agent_keys,
            max_session_cost_usd,
            max_session_minutes,
            observed_cost_usd: cost_ledger.total_observed_cost_usd,
            links_path: evidence.links_path.as_ref(),
            attachments_manifest_path: evidence.attachments_manifest_path.as_ref(),
            human_log_path: &human_log_path,
        };
        return Ok(editorial_session_result(
            &context,
            None,
            &minutes_path,
            current_draft_path,
            agents,
            false,
            "PAUSED_DRAFT_UNAVAILABLE",
        ));
    }

    let round_turn_specs = circular_round_turn_specs(draft_lead_key, &active_agent_keys);
    let round_turn_count = round_turn_specs.len();
    if round_turn_count < 2 {
        let _ = write_log_record(
            log_session,
            LogEventInput {
                level: "error".to_string(),
                category: "session.review.no_independent_reviewer".to_string(),
                message: "no circular review peer remains after selecting the cycle lead"
                    .to_string(),
                context: Some(json!({
                    "run_id": &run_id,
                    "round": round,
                    "cycle_lead": draft_lead_key,
                    "active_agents": active_agent_keys.clone(),
                    "policy": "circular_round_requires_at_least_two_agents"
                })),
            },
        );
        let minutes_path = session_dir.join("ata-da-sessao.md");
        write_text_file(
            &minutes_path,
            &build_session_minutes(request, &run_id, &agents, false, None),
        )?;
        let context = SessionResultContext {
            run_id: &run_id,
            session_dir: &session_dir,
            prompt_path: &prompt_path,
            protocol_path: &protocol_path,
            active_agents: &active_agent_keys,
            max_session_cost_usd,
            max_session_minutes,
            observed_cost_usd: cost_ledger.total_observed_cost_usd,
            links_path: evidence.links_path.as_ref(),
            attachments_manifest_path: evidence.attachments_manifest_path.as_ref(),
            human_log_path: &human_log_path,
        };
        return Ok(editorial_session_result(
            &context,
            None,
            &minutes_path,
            current_draft_path,
            agents,
            false,
            "PAUSED_REVIEWERS_UNAVAILABLE",
        ));
    }
    if is_resume && !current_draft.trim().is_empty() {
        let progress = restore_circular_resume_progress(
            &agent_dir,
            current_draft_path.as_ref(),
            &agents,
            round,
            &round_turn_specs,
        );
        round = progress.round;
        round_turn_index = progress.turn_index;
        valid_round_agents = progress.valid_agents;
        round_had_substantive_change = progress.had_substantive_change;
        round_had_editorial_divergence = progress.had_editorial_divergence;
        let _ = write_log_record(
            log_session,
            LogEventInput {
                level: "info".to_string(),
                category: "session.resume.circular_progress_restored".to_string(),
                message: "circular review progress restored from existing artifacts".to_string(),
                context: Some(json!({
                    "run_id": &run_id,
                    "round": round,
                    "next_turn": round_turn_index + 1,
                    "round_turn_count": round_turn_count,
                    "valid_round_agents": valid_round_agents.iter().cloned().collect::<Vec<_>>(),
                    "round_had_substantive_change": round_had_substantive_change,
                    "round_had_editorial_divergence": round_had_editorial_divergence,
                    "policy": "resume_continues_the_circular_circuit_without_self_review"
                })),
            },
        );
    }

    let final_path: PathBuf;
    loop {
        // Operator-driven stop check at the top of every turn. Granularity
        // is "between turns" for the orchestration; in-flight CLI peer is
        // killed via `command_spawn::run_resolved_command_observed` 250ms
        // poll; in-flight API peer is dropped via `tokio::select!` in
        // `provider_retry::send_with_retry_async`.
        if cancel_token.is_cancelled() {
            let _ = write_log_record(
                log_session,
                LogEventInput {
                    level: "warn".to_string(),
                    category: "session.user.stop_completed".to_string(),
                    message: "editorial session stopped by operator between turns".to_string(),
                    context: Some(json!({
                        "run_id": run_id.clone(),
                        "round": round,
                        "agents_so_far": agents.len()
                    })),
                },
            );
            let minutes_path = session_dir.join("ata-da-sessao.md");
            write_text_file(
                &minutes_path,
                &build_session_minutes(request, &run_id, &agents, false, None),
            )?;
            let context = SessionResultContext {
                run_id: &run_id,
                session_dir: &session_dir,
                prompt_path: &prompt_path,
                protocol_path: &protocol_path,
                active_agents: &active_agent_keys,
                max_session_cost_usd,
                max_session_minutes,
                observed_cost_usd: cost_ledger.total_observed_cost_usd,
                links_path: evidence.links_path.as_ref(),
                attachments_manifest_path: evidence.attachments_manifest_path.as_ref(),
                human_log_path: &human_log_path,
            };
            return Ok(editorial_session_result(
                &context,
                None,
                &minutes_path,
                current_draft_path,
                agents,
                false,
                "STOPPED_BY_USER",
            ));
        }
        if session_time_exhausted(time_budget_anchor, max_session_minutes) {
            let minutes_path = session_dir.join("ata-da-sessao.md");
            write_text_file(
                &minutes_path,
                &build_session_minutes(request, &run_id, &agents, false, None),
            )?;
            let context = SessionResultContext {
                run_id: &run_id,
                session_dir: &session_dir,
                prompt_path: &prompt_path,
                protocol_path: &protocol_path,
                active_agents: &active_agent_keys,
                max_session_cost_usd,
                max_session_minutes,
                observed_cost_usd: cost_ledger.total_observed_cost_usd,
                links_path: evidence.links_path.as_ref(),
                attachments_manifest_path: evidence.attachments_manifest_path.as_ref(),
                human_log_path: &human_log_path,
            };
            return Ok(editorial_session_result(
                &context,
                None,
                &minutes_path,
                current_draft_path,
                agents,
                false,
                "TIME_LIMIT_REACHED",
            ));
        }
        let draft_author_key = current_draft_author_key.as_deref().unwrap_or("unknown");
        let max_serial_turns = std::cmp::max(round_turn_count * 4, round_turn_count);
        serial_turns += 1;
        if serial_turns > max_serial_turns {
            let _ = write_log_record(
                log_session,
                LogEventInput {
                    level: "warn".to_string(),
                    category: "session.serial.turn_cap_reached".to_string(),
                    message: "serial editorial cycle paused after the configured hard turn cap"
                        .to_string(),
                    context: Some(json!({
                        "run_id": &run_id,
                        "round": round,
                        "turn": round_turn_index + 1,
                        "serial_turns": serial_turns,
                        "max_serial_turns": max_serial_turns,
                        "stable_serial_approvals": stable_serial_approval_agents.len(),
                        "policy": "avoid_infinite_serial_deliberation"
                    })),
                },
            );
            let minutes_path = session_dir.join("ata-da-sessao.md");
            write_text_file(
                &minutes_path,
                &build_session_minutes(request, &run_id, &agents, false, None),
            )?;
            let context = SessionResultContext {
                run_id: &run_id,
                session_dir: &session_dir,
                prompt_path: &prompt_path,
                protocol_path: &protocol_path,
                active_agents: &active_agent_keys,
                max_session_cost_usd,
                max_session_minutes,
                observed_cost_usd: cost_ledger.total_observed_cost_usd,
                links_path: evidence.links_path.as_ref(),
                attachments_manifest_path: evidence.attachments_manifest_path.as_ref(),
                human_log_path: &human_log_path,
            };
            return Ok(editorial_session_result(
                &context,
                None,
                &minutes_path,
                current_draft_path,
                agents,
                false,
                "PAUSED_EDITORIAL_CYCLE_LIMIT",
            ));
        }

        let spec = round_turn_specs[round_turn_index];
        let closing_turn = spec.key == draft_lead_key;
        let valid_agents_required_before_closure = round_turn_specs
            .iter()
            .filter(|turn_spec| turn_spec.key != draft_lead_key)
            .count();
        if closing_turn && valid_round_agents.len() < valid_agents_required_before_closure {
            let _ = write_log_record(
                log_session,
                LogEventInput {
                    level: "warn".to_string(),
                    category: "session.serial.round_incomplete_before_closure".to_string(),
                    message: "circular round cannot return to the original redactor before every other peer completes a valid turn".to_string(),
                    context: Some(json!({
                        "run_id": &run_id,
                        "round": round,
                        "turn": round_turn_index + 1,
                        "cycle_lead": draft_lead_key,
                        "valid_round_agents": valid_round_agents.iter().cloned().collect::<Vec<_>>(),
                        "required_valid_peer_count_before_closure": valid_agents_required_before_closure,
                        "policy": "round_closes_only_after_full_peer_circuit"
                    })),
                },
            );
            let minutes_path = session_dir.join("ata-da-sessao.md");
            write_text_file(
                &minutes_path,
                &build_session_minutes(request, &run_id, &agents, false, None),
            )?;
            let context = SessionResultContext {
                run_id: &run_id,
                session_dir: &session_dir,
                prompt_path: &prompt_path,
                protocol_path: &protocol_path,
                active_agents: &active_agent_keys,
                max_session_cost_usd,
                max_session_minutes,
                observed_cost_usd: cost_ledger.total_observed_cost_usd,
                links_path: evidence.links_path.as_ref(),
                attachments_manifest_path: evidence.attachments_manifest_path.as_ref(),
                human_log_path: &human_log_path,
            };
            return Ok(editorial_session_result(
                &context,
                None,
                &minutes_path,
                current_draft_path,
                agents,
                false,
                "PAUSED_ROUND_INCOMPLETE",
            ));
        }

        if spec.key == draft_author_key && !(closing_turn && !round_had_substantive_change) {
            let _ = write_log_record(
                log_session,
                LogEventInput {
                    level: "error".to_string(),
                    category: "session.tribunal.self_review_blocked".to_string(),
                    message:
                        "serial scheduler attempted to assign the current version to its own author"
                            .to_string(),
                    context: Some(json!({
                        "run_id": &run_id,
                        "round": round,
                        "turn": round_turn_index + 1,
                        "reviewer": spec.key,
                        "current_author": draft_author_key,
                        "closing_turn": closing_turn,
                        "policy": "agent_never_reviews_own_current_version"
                    })),
                },
            );
            let minutes_path = session_dir.join("ata-da-sessao.md");
            write_text_file(
                &minutes_path,
                &build_session_minutes(request, &run_id, &agents, false, None),
            )?;
            let context = SessionResultContext {
                run_id: &run_id,
                session_dir: &session_dir,
                prompt_path: &prompt_path,
                protocol_path: &protocol_path,
                active_agents: &active_agent_keys,
                max_session_cost_usd,
                max_session_minutes,
                observed_cost_usd: cost_ledger.total_observed_cost_usd,
                links_path: evidence.links_path.as_ref(),
                attachments_manifest_path: evidence.attachments_manifest_path.as_ref(),
                human_log_path: &human_log_path,
            };
            return Ok(editorial_session_result(
                &context,
                None,
                &minutes_path,
                current_draft_path,
                agents,
                false,
                "PAUSED_SELF_REVIEW_BLOCKED",
            ));
        }

        if let Some(author_key) = current_draft_author_key.as_deref() {
            let _ = write_log_record(
                log_session,
                LogEventInput {
                    level: "info".to_string(),
                    category: "session.serial.author_excluded".to_string(),
                    message: "current version author excluded from serial reviewer-reviser turn"
                        .to_string(),
                    context: Some(json!({
                        "run_id": &run_id,
                        "round": round,
                        "turn": round_turn_index + 1,
                        "draft_author_key": author_key,
                        "review_agents": round_turn_specs.iter().map(|spec| spec.key).collect::<Vec<_>>(),
                        "policy": "agent_never_reviews_own_current_version"
                    })),
                },
            );
        }
        let previous_revision_history = build_revision_history_block(&agents);
        let review_prompt = build_serial_revision_prompt(
            request,
            &run_id,
            round_turn_index + 1,
            &current_draft,
            draft_author_key,
            spec.key,
            closing_turn,
            &previous_revision_history,
            &evidence.block,
        );
        let _ = write_log_record(
            log_session,
            LogEventInput {
                level: "info".to_string(),
                category: "session.serial.reviewer_selected".to_string(),
                message: "serial reviewer-reviser selected for the current version".to_string(),
                context: Some(json!({
                    "run_id": &run_id,
                    "round": round,
                    "turn": round_turn_index + 1,
                    "reviewer": spec.key,
                    "current_author": draft_author_key,
                    "stable_serial_approvals": stable_serial_approval_agents.len(),
                    "round_turn_count": round_turn_count,
                    "closing_turn": closing_turn,
                    "policy": "one_peer_revises_then_passes_to_next_peer_no_self_review_round_closes_on_return_to_redactor"
                })),
            },
        );
        if let Some(max_cost_usd) = max_session_cost_usd {
            let projected_round_cost_usd = if api_agent_keys.contains(spec.key) {
                api_provider_for_agent(spec.key)
                    .and_then(|provider| {
                        let rates = provider_cost_rates.get(spec.key).copied()?;
                        let input_estimate_chars = api_input_estimate_chars(
                            &review_prompt,
                            &evidence.attachments,
                            provider,
                        );
                        Some(estimate_provider_cost_from_input_chars(
                            input_estimate_chars,
                            api_role_max_tokens("review"),
                            rates,
                        ))
                    })
                    .unwrap_or(0.0)
            } else {
                0.0
            };
            if projected_round_cost_usd > 0.0
                && cost_ledger.total_observed_cost_usd + projected_round_cost_usd > max_cost_usd
            {
                let _ = write_log_record(
                    log_session,
                    LogEventInput {
                        level: "warn".to_string(),
                        category: "session.cost.serial_turn_blocked".to_string(),
                        message: "serial reviewer-reviser turn not started because remaining budget cannot cover it".to_string(),
                        context: Some(json!({
                            "run_id": &run_id,
                            "round": round,
                            "observed_cost_usd": cost_ledger.total_observed_cost_usd,
                            "projected_review_turn_cost_usd": projected_round_cost_usd,
                            "max_session_cost_usd": max_cost_usd,
                            "review_agent": spec.key,
                            "policy": "do_not_start_paid_serial_turn_when_cost_cap_would_interrupt_it"
                        })),
                    },
                );
                let minutes_path = session_dir.join("ata-da-sessao.md");
                write_text_file(
                    &minutes_path,
                    &build_session_minutes(request, &run_id, &agents, false, None),
                )?;
                let context = SessionResultContext {
                    run_id: &run_id,
                    session_dir: &session_dir,
                    prompt_path: &prompt_path,
                    protocol_path: &protocol_path,
                    active_agents: &active_agent_keys,
                    max_session_cost_usd,
                    max_session_minutes,
                    observed_cost_usd: cost_ledger.total_observed_cost_usd,
                    links_path: evidence.links_path.as_ref(),
                    attachments_manifest_path: evidence.attachments_manifest_path.as_ref(),
                    human_log_path: &human_log_path,
                };
                return Ok(editorial_session_result(
                    &context,
                    None,
                    &minutes_path,
                    current_draft_path,
                    agents,
                    false,
                    "COST_LIMIT_REACHED",
                ));
            }
        }
        let output_path = agent_attempt_output_path(&agent_dir, round, spec.key, "revision");
        let timeout = remaining_session_duration(time_budget_anchor, max_session_minutes);
        let use_api_agent = api_agent_keys.contains(spec.key);
        let cost_guard = if use_api_agent {
            provider_cost_guard_for(
                max_session_cost_usd,
                provider_cost_rates.get(spec.key).copied(),
                &cost_ledger,
            )
        } else {
            None
        };
        let mut result = run_editorial_agent_for_spec(
            log_session,
            &run_id,
            spec,
            "review",
            review_prompt,
            &evidence.attachments,
            &output_path,
            timeout,
            &ai_provider_config,
            cost_guard,
            use_api_agent,
            cancel_token,
        );
        append_agent_cost_to_ledger(&session_dir, &mut cost_ledger, &cost_scope_id, &result)?;
        agents.push(result.clone());

        if result.status == "COST_LIMIT_REACHED" {
            let minutes_path = session_dir.join("ata-da-sessao.md");
            write_text_file(
                &minutes_path,
                &build_session_minutes(request, &run_id, &agents, false, None),
            )?;
            let context = SessionResultContext {
                run_id: &run_id,
                session_dir: &session_dir,
                prompt_path: &prompt_path,
                protocol_path: &protocol_path,
                active_agents: &active_agent_keys,
                max_session_cost_usd,
                max_session_minutes,
                observed_cost_usd: cost_ledger.total_observed_cost_usd,
                links_path: evidence.links_path.as_ref(),
                attachments_manifest_path: evidence.attachments_manifest_path.as_ref(),
                human_log_path: &human_log_path,
            };
            return Ok(editorial_session_result(
                &context,
                None,
                &minutes_path,
                current_draft_path,
                agents,
                false,
                "COST_LIMIT_REACHED",
            ));
        }

        if result.tone == "error" || result.tone == "blocked" {
            consecutive_reviewer_outage_rounds += 1;
            let _ = write_log_record(
                log_session,
                LogEventInput {
                    level: "warn".to_string(),
                    category: "session.serial.operational_failure".to_string(),
                    message: "serial reviewer-reviser turn had an operational failure; keeping current text and moving on".to_string(),
                    context: Some(json!({
                        "run_id": &run_id,
                        "round": round,
                        "agent": result.name,
                        "status": result.status,
                        "consecutive_reviewer_outage_rounds": consecutive_reviewer_outage_rounds,
                        "policy": "operational_failure_does_not_modify_editorial_text"
                    })),
                },
            );
            if consecutive_reviewer_outage_rounds >= ALL_ERROR_ESCALATION_THRESHOLD {
                let _ = write_log_record(
                    log_session,
                    LogEventInput {
                        level: "error".to_string(),
                        category: "session.escalation.reviewer_operational_outage".to_string(),
                        message: "serial reviewer-reviser turns failed operationally across N consecutive attempts; pausing recoverably for operator review".to_string(),
                        context: Some(json!({
                            "run_id": &run_id,
                            "round": round,
                            "draft_author_key": current_draft_author_key.clone(),
                            "consecutive_reviewer_outage_rounds": consecutive_reviewer_outage_rounds,
                            "threshold": ALL_ERROR_ESCALATION_THRESHOLD,
                            "policy": "recoverable_pause_no_self_review_no_text_revision"
                        })),
                    },
                );
                let minutes_path = session_dir.join("ata-da-sessao.md");
                write_text_file(
                    &minutes_path,
                    &build_session_minutes(request, &run_id, &agents, false, None),
                )?;
                let context = SessionResultContext {
                    run_id: &run_id,
                    session_dir: &session_dir,
                    prompt_path: &prompt_path,
                    protocol_path: &protocol_path,
                    active_agents: &active_agent_keys,
                    max_session_cost_usd,
                    max_session_minutes,
                    observed_cost_usd: cost_ledger.total_observed_cost_usd,
                    links_path: evidence.links_path.as_ref(),
                    attachments_manifest_path: evidence.attachments_manifest_path.as_ref(),
                    human_log_path: &human_log_path,
                };
                return Ok(editorial_session_result(
                    &context,
                    None,
                    &minutes_path,
                    current_draft_path,
                    agents,
                    false,
                    "PAUSED_REVIEWER_OPERATIONAL_OUTAGE",
                ));
            }
            round_turn_index += 1;
            if round_turn_index >= round_turn_count {
                let minutes_path = session_dir.join("ata-da-sessao.md");
                write_text_file(
                    &minutes_path,
                    &build_session_minutes(request, &run_id, &agents, false, None),
                )?;
                let context = SessionResultContext {
                    run_id: &run_id,
                    session_dir: &session_dir,
                    prompt_path: &prompt_path,
                    protocol_path: &protocol_path,
                    active_agents: &active_agent_keys,
                    max_session_cost_usd,
                    max_session_minutes,
                    observed_cost_usd: cost_ledger.total_observed_cost_usd,
                    links_path: evidence.links_path.as_ref(),
                    attachments_manifest_path: evidence.attachments_manifest_path.as_ref(),
                    human_log_path: &human_log_path,
                };
                return Ok(editorial_session_result(
                    &context,
                    None,
                    &minutes_path,
                    current_draft_path,
                    agents,
                    false,
                    "PAUSED_ROUND_INCOMPLETE",
                ));
            }
            continue;
        }
        consecutive_reviewer_outage_rounds = 0;

        let artifact = read_text_file(&output_path).unwrap_or_default();
        let stdout = extract_stdout_block(&artifact).unwrap_or(artifact.as_str());
        let serial_output = match validate_serial_turn_output(stdout, &result.status) {
            Ok(output) => output,
            Err(reason) => {
                result.status = "CONTRACT_VIOLATION".to_string();
                result.tone = "error".to_string();
                if let Some(last) = agents.last_mut() {
                    last.status = result.status.clone();
                    last.tone = result.tone.clone();
                }
                reclassify_agent_artifact_status(&output_path, "CONTRACT_VIOLATION", &reason);
                consecutive_reviewer_outage_rounds += 1;
                stable_serial_approval_agents.clear();
                round_had_editorial_divergence = true;
                let _ = write_log_record(
                    log_session,
                    LogEventInput {
                        level: "warn".to_string(),
                        category: "session.serial.contract_violation".to_string(),
                        message:
                            "serial reviewer-reviser returned an incomplete or unusable output contract"
                                .to_string(),
                        context: Some(json!({
                            "run_id": &run_id,
                            "round": round,
                            "turn": round_turn_index + 1,
                            "agent": spec.key,
                            "status": result.status,
                            "reason": reason,
                            "policy": "strict_output_contract_required_for_serial_custody_or_approval"
                        })),
                    },
                );
                round_turn_index += 1;
                if round_turn_index >= round_turn_count {
                    let minutes_path = session_dir.join("ata-da-sessao.md");
                    write_text_file(
                        &minutes_path,
                        &build_session_minutes(request, &run_id, &agents, false, None),
                    )?;
                    let context = SessionResultContext {
                        run_id: &run_id,
                        session_dir: &session_dir,
                        prompt_path: &prompt_path,
                        protocol_path: &protocol_path,
                        active_agents: &active_agent_keys,
                        max_session_cost_usd,
                        max_session_minutes,
                        observed_cost_usd: cost_ledger.total_observed_cost_usd,
                        links_path: evidence.links_path.as_ref(),
                        attachments_manifest_path: evidence.attachments_manifest_path.as_ref(),
                        human_log_path: &human_log_path,
                    };
                    return Ok(editorial_session_result(
                        &context,
                        None,
                        &minutes_path,
                        current_draft_path,
                        agents,
                        false,
                        "PAUSED_ROUND_INCOMPLETE",
                    ));
                }
                continue;
            }
        };
        valid_round_agents.insert(spec.key.to_string());
        let Some(revised_text) = serial_output.final_text else {
            let _ = write_log_record(
                log_session,
                LogEventInput {
                    level: "info".to_string(),
                    category: "session.serial.stable_approval".to_string(),
                    message:
                        "serial reviewer-reviser approved or objected without changing custody"
                            .to_string(),
                    context: Some(json!({
                        "run_id": &run_id,
                        "round": round,
                        "turn": round_turn_index + 1,
                        "agent": spec.key,
                        "status": result.status,
                        "stable_serial_approvals": stable_serial_approval_agents.len(),
                        "policy": "unchanged_turn_does_not_transfer_text_custody"
                    })),
                },
            );
            if result.status == "READY" {
                stable_serial_approval_agents.insert(spec.key.to_string());
            } else {
                round_had_editorial_divergence = true;
                stable_serial_approval_agents.clear();
            }
            round_turn_index += 1;
            if round_turn_index >= round_turn_count {
                if !round_had_substantive_change
                    && !round_had_editorial_divergence
                    && valid_round_agents.len() >= round_turn_count
                {
                    let path = session_dir.join("texto-final.md");
                    write_text_file(&path, &strip_leading_maestro_status(&current_draft))?;
                    final_path = path;
                    break;
                }
                round += 1;
                round_turn_index = 0;
                valid_round_agents.clear();
                stable_serial_approval_agents.clear();
                round_had_substantive_change = false;
                round_had_editorial_divergence = false;
            }
            continue;
        };

        let substantive_change = is_substantive_editorial_change(&current_draft, &revised_text);
        if quality_guard_blocks_revision(
            current_draft_author_key.as_deref(),
            spec.key,
            &current_draft,
            &revised_text,
            substantive_change,
        ) {
            let _ = write_log_record(
                log_session,
                LogEventInput {
                    level: "warn".to_string(),
                    category: "session.serial.quality_guard_blocked".to_string(),
                    message: "serial revision rejected because a lower-tier peer materially shrank a stronger formulation without enough protection".to_string(),
                    context: Some(json!({
                        "run_id": &run_id,
                        "round": round,
                        "turn": round_turn_index + 1,
                        "reviewer": spec.key,
                        "current_author": current_draft_author_key.clone(),
                        "current_chars": current_draft.chars().count(),
                        "revised_chars": revised_text.chars().count(),
                        "policy": "anti_impoverishment_quality_ratchet"
                    })),
                },
            );
            stable_serial_approval_agents.clear();
            round_had_editorial_divergence = true;
            round_turn_index += 1;
            if round_turn_index >= round_turn_count {
                round += 1;
                round_turn_index = 0;
                valid_round_agents.clear();
                stable_serial_approval_agents.clear();
                round_had_substantive_change = false;
                round_had_editorial_divergence = false;
            }
            continue;
        }

        if substantive_change {
            current_draft = revised_text;
            current_draft_path = Some(output_path);
            current_draft_author_key = Some(spec.key.to_string());
            stable_serial_approval_agents.clear();
            round_had_substantive_change = true;
            let _ = write_log_record(
                log_session,
                LogEventInput {
                    level: "info".to_string(),
                    category: "session.serial.version_advanced".to_string(),
                    message: "serial reviewer-reviser produced a new current version".to_string(),
                    context: Some(json!({
                        "run_id": &run_id,
                        "round": round,
                        "turn": round_turn_index + 1,
                        "author": spec.key,
                        "status": result.status,
                        "policy": "new_version_requires_full_independent_rotation"
                    })),
                },
            );
        } else if result.status == "READY" {
            stable_serial_approval_agents.insert(spec.key.to_string());
            let _ = write_log_record(
                log_session,
                LogEventInput {
                    level: "info".to_string(),
                    category: "session.serial.stable_approval".to_string(),
                    message: "serial reviewer-reviser approved the current version without substantive changes".to_string(),
                    context: Some(json!({
                        "run_id": &run_id,
                        "round": round,
                        "turn": round_turn_index + 1,
                        "reviewer": spec.key,
                        "stable_serial_approvals": stable_serial_approval_agents.len(),
                        "stable_serial_approval_agents": stable_serial_approval_agents.iter().cloned().collect::<Vec<_>>(),
                        "required_stable_approvals": round_turn_count,
                        "policy": "converge_after_full_rotation_without_substantive_change"
                    })),
                },
            );
        } else {
            stable_serial_approval_agents.clear();
            round_had_editorial_divergence = true;
        }

        round_turn_index += 1;
        if round_turn_index >= round_turn_count {
            if !round_had_substantive_change
                && !round_had_editorial_divergence
                && valid_round_agents.len() >= round_turn_count
            {
                let path = session_dir.join("texto-final.md");
                write_text_file(&path, &strip_leading_maestro_status(&current_draft))?;
                final_path = path;
                break;
            }
            round += 1;
            round_turn_index = 0;
            valid_round_agents.clear();
            stable_serial_approval_agents.clear();
            round_had_substantive_change = false;
            round_had_editorial_divergence = false;
        }
    }

    let minutes_path = session_dir.join("ata-da-sessao.md");
    write_text_file(
        &minutes_path,
        &build_session_minutes(request, &run_id, &agents, true, Some(&final_path)),
    )?;

    let context = SessionResultContext {
        run_id: &run_id,
        session_dir: &session_dir,
        prompt_path: &prompt_path,
        protocol_path: &protocol_path,
        active_agents: &active_agent_keys,
        max_session_cost_usd,
        max_session_minutes,
        observed_cost_usd: cost_ledger.total_observed_cost_usd,
        links_path: evidence.links_path.as_ref(),
        attachments_manifest_path: evidence.attachments_manifest_path.as_ref(),
        human_log_path: &human_log_path,
    };
    Ok(editorial_session_result(
        &context,
        Some(&final_path),
        &minutes_path,
        current_draft_path,
        agents,
        true,
        "READY_UNANIMOUS",
    ))
}

fn current_draft_author_from_path(agent_dir: &Path, path: Option<&PathBuf>) -> Option<String> {
    let name = path?.file_name()?.to_str()?;
    let artifact = parse_agent_artifact_name(agent_dir, name)?;
    if matches!(artifact.role.as_str(), "draft" | "revision") {
        Some(artifact.agent)
    } else {
        None
    }
}

fn circular_round_turn_specs(
    first_key: &str,
    active_agents: &[String],
) -> Vec<crate::EditorialAgentSpec> {
    let mut specs = selected_editorial_agent_specs(first_key, active_agents);
    if specs.len() > 1 {
        specs.rotate_left(1);
    }
    specs
}

struct CircularResumeProgress {
    round: usize,
    turn_index: usize,
    valid_agents: BTreeSet<String>,
    had_substantive_change: bool,
    had_editorial_divergence: bool,
}

fn restore_circular_resume_progress(
    agent_dir: &Path,
    current_draft_path: Option<&PathBuf>,
    agents: &[EditorialAgentResult],
    fallback_round: usize,
    round_turn_specs: &[crate::EditorialAgentSpec],
) -> CircularResumeProgress {
    let mut progress = CircularResumeProgress {
        round: fallback_round.max(1),
        turn_index: 0,
        valid_agents: BTreeSet::new(),
        had_substantive_change: false,
        had_editorial_divergence: false,
    };
    let Some(path) = current_draft_path else {
        return progress;
    };
    let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
        return progress;
    };
    let Some(current_artifact) = parse_agent_artifact_name(agent_dir, name) else {
        return progress;
    };

    progress.round = current_artifact.round.max(1);
    if current_artifact.role == "revision" {
        progress.had_substantive_change = true;
        if let Some(index) = round_turn_specs
            .iter()
            .position(|spec| spec.key == current_artifact.agent)
        {
            progress.turn_index = index + 1;
        }
    }
    if progress.turn_index >= round_turn_specs.len() {
        progress.round += 1;
        progress.turn_index = 0;
        progress.had_substantive_change = false;
        return progress;
    }

    for agent in agents {
        let path = Path::new(&agent.output_path);
        let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
            continue;
        };
        let Some(artifact) = parse_agent_artifact_name(agent_dir, name) else {
            continue;
        };
        if artifact.round != progress.round
            || !matches!(artifact.role.as_str(), "revision" | "review")
        {
            continue;
        }
        let Some(index) = round_turn_specs
            .iter()
            .position(|spec| spec.key == artifact.agent)
        else {
            continue;
        };
        if index >= progress.turn_index {
            continue;
        }
        let artifact_text = read_text_file(&artifact.path).unwrap_or_default();
        let stdout = extract_stdout_block(&artifact_text).unwrap_or(artifact_text.as_str());
        if validate_serial_turn_output(stdout, &agent.status).is_err() {
            continue;
        }
        progress.valid_agents.insert(artifact.agent.clone());
        if agent.status != "READY" {
            progress.had_editorial_divergence = true;
        }
    }
    progress
}

#[derive(Debug)]
struct SerialTurnOutput {
    final_text: Option<String>,
}

fn validate_serial_turn_output(stdout: &str, status: &str) -> Result<SerialTurnOutput, String> {
    if status != "READY" && status != "NOT_READY" {
        return Err(format!("invalid serial status: {status}"));
    }
    if contains_prompt_or_protocol_echo(stdout) {
        return Err("output appears to reproduce prompt/protocol scaffolding".to_string());
    }
    require_balanced_tag(stdout, "maestro_revision_report")?;
    require_balanced_optional_tag(stdout, "maestro_final_text")?;

    let Some(report) = extract_tagged_block(stdout, "maestro_revision_report") else {
        return Err("missing complete maestro_revision_report block".to_string());
    };
    if report.trim().is_empty() {
        return Err("empty maestro_revision_report block".to_string());
    }
    let final_text = extract_tagged_block(stdout, "maestro_final_text");
    let has_revised_custody = report_declares_custody_value(&report, "revised");
    let has_unchanged_custody = report_declares_custody_value(&report, "unchanged");
    if has_revised_custody && has_unchanged_custody {
        return Err("ambiguous custody declaration in maestro_revision_report".to_string());
    }
    if let Some(text) = final_text.as_ref() {
        if text.trim().is_empty() {
            return Err("empty maestro_final_text block".to_string());
        }
        if !has_revised_custody {
            return Err("maestro_final_text requires custody revised in the report".to_string());
        }
    }
    if status == "READY" && final_text.is_none() && !has_unchanged_custody {
        return Err(
            "READY without maestro_final_text must explicitly declare custody unchanged"
                .to_string(),
        );
    }
    if final_text.is_none() && has_revised_custody {
        return Err("revised custody requires a complete maestro_final_text block".to_string());
    }
    Ok(SerialTurnOutput { final_text })
}

fn require_balanced_tag(stdout: &str, tag: &str) -> Result<(), String> {
    let open = stdout.matches(&format!("<{tag}>")).count();
    let close = stdout.matches(&format!("</{tag}>")).count();
    if open == 1 && close == 1 {
        Ok(())
    } else if open == 0 && close == 0 {
        Err(format!("missing {tag} block"))
    } else {
        Err(format!("incomplete or duplicated {tag} block"))
    }
}

fn require_balanced_optional_tag(stdout: &str, tag: &str) -> Result<(), String> {
    let open = stdout.matches(&format!("<{tag}>")).count();
    let close = stdout.matches(&format!("</{tag}>")).count();
    if open == close && open <= 1 {
        Ok(())
    } else {
        Err(format!("incomplete or duplicated {tag} block"))
    }
}

fn report_declares_custody_value(report: &str, value: &str) -> bool {
    let normalized = report.to_ascii_lowercase();
    let value = value.to_ascii_lowercase();
    [
        format!("\"custody\": \"{value}\""),
        format!("\"custody\":\"{value}\""),
        format!("'custody': '{value}'"),
        format!("'custody':'{value}'"),
        format!("custody: \"{value}\""),
        format!("custody:\"{value}\""),
        format!("custody: `{value}`"),
        format!("custody:`{value}`"),
        format!("custody: {value}"),
        format!("custody:{value}"),
    ]
    .iter()
    .any(|pattern| normalized.contains(pattern))
}

fn contains_prompt_or_protocol_echo(stdout: &str) -> bool {
    let normalized = stdout.to_ascii_lowercase();
    [
        "# maestro editorial ai - serial review-rewrite turn",
        "## full editorial protocol",
        "## required output contract",
        "## sovereign approved-content lock",
        "## current text under custody",
        "## prior serial revision reports",
        "internal coordination, critique, changelog, and revision report",
    ]
    .iter()
    .any(|marker| normalized.contains(marker))
}

fn reclassify_agent_artifact_status(output_path: &Path, status: &str, reason: &str) {
    let Ok(contents) = read_text_file(output_path) else {
        return;
    };
    let mut replaced = false;
    let mut rewritten = Vec::new();
    for line in contents.lines() {
        if !replaced && line.trim_start().starts_with("- Status: `") {
            rewritten.push(format!("- Status: `{status}`"));
            replaced = true;
        } else {
            rewritten.push(line.to_string());
        }
    }
    if !replaced {
        rewritten.insert(0, format!("- Status: `{status}`"));
    }
    let mut text = rewritten.join("\n");
    if !text.contains("Reclassificado para CONTRACT_VIOLATION") {
        text.push_str(&format!(
            "\n> Reclassificado para CONTRACT_VIOLATION: {}.\n",
            sanitize_text(reason, 300)
        ));
    }
    let _ = write_text_file(output_path, &text);
}

#[cfg(test)]
fn is_operational_only_review_round(round_results: &[EditorialAgentResult]) -> bool {
    !round_results.is_empty()
        && round_results.iter().all(|agent| {
            agent.role == "review" && agent.status != "READY" && is_operational_agent_result(agent)
        })
}

fn normalized_editorial_text(text: &str) -> String {
    text.replace("\r\n", "\n")
        .replace('\r', "\n")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string()
}

fn is_substantive_editorial_change(before: &str, after: &str) -> bool {
    normalized_editorial_text(before) != normalized_editorial_text(after)
}

fn editorial_quality_tier(agent_key: &str) -> u8 {
    match agent_key.to_ascii_lowercase().as_str() {
        "claude" | "codex" => 3,
        "gemini" => 2,
        "deepseek" | "grok" => 1,
        _ => 0,
    }
}

fn quality_guard_blocks_revision(
    current_author_key: Option<&str>,
    reviewer_key: &str,
    before: &str,
    after: &str,
    substantive_change: bool,
) -> bool {
    if !substantive_change {
        return false;
    }
    let Some(author_key) = current_author_key else {
        return false;
    };
    if editorial_quality_tier(reviewer_key) >= editorial_quality_tier(author_key) {
        return false;
    }
    let before_chars = before.chars().count();
    let after_chars = after.chars().count();
    before_chars >= 400 && after_chars * 100 < before_chars * 85
}

fn agent_attempt_output_path(agent_dir: &Path, round: usize, agent: &str, role: &str) -> PathBuf {
    let canonical = agent_dir.join(format!("round-{round:03}-{agent}-{role}.md"));
    if !canonical.exists() {
        return canonical;
    }

    for attempt in 2..=9999 {
        let candidate = agent_dir.join(format!(
            "round-{round:03}-{agent}-{role}-attempt-{attempt:03}.md"
        ));
        if !candidate.exists() {
            return candidate;
        }
    }

    let fallback_attempt = 10_000 + Utc::now().timestamp_millis().rem_euclid(1_000_000) as usize;
    agent_dir.join(format!(
        "round-{round:03}-{agent}-{role}-attempt-{fallback_attempt}.md"
    ))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{
        agent_attempt_output_path, circular_round_turn_specs, current_draft_author_from_path,
        is_operational_only_review_round, is_substantive_editorial_change,
        quality_guard_blocks_revision, restore_circular_resume_progress,
        validate_serial_turn_output,
    };
    use crate::EditorialAgentResult;

    fn review_result(name: &str, status: &str, tone: &str) -> EditorialAgentResult {
        EditorialAgentResult {
            name: name.to_string(),
            role: "review".to_string(),
            cli: name.to_ascii_lowercase(),
            tone: tone.to_string(),
            status: status.to_string(),
            duration_ms: 1,
            exit_code: Some(1),
            output_path: format!(
                "agent-runs/round-001-{}-review.md",
                name.to_ascii_lowercase()
            ),
            usage_input_tokens: None,
            usage_output_tokens: None,
            cost_usd: None,
            cost_estimated: None,
            cache: None,
        }
    }

    #[test]
    fn resume_author_recovery_reads_latest_draft_or_revision_artifact() {
        let agent_dir = PathBuf::from("agent-runs");

        assert_eq!(
            current_draft_author_from_path(
                &agent_dir,
                Some(&PathBuf::from("agent-runs/round-072-codex-draft.md")),
            ),
            Some("codex".to_string())
        );
        assert_eq!(
            current_draft_author_from_path(
                &agent_dir,
                Some(&PathBuf::from("agent-runs/round-073-claude-revision.md")),
            ),
            Some("claude".to_string())
        );
        assert_eq!(
            current_draft_author_from_path(
                &agent_dir,
                Some(&PathBuf::from(
                    "agent-runs/round-073-claude-revision-attempt-002.md"
                )),
            ),
            Some("claude".to_string())
        );
    }

    #[test]
    fn resume_author_recovery_ignores_review_or_invalid_artifacts() {
        let agent_dir = PathBuf::from("agent-runs");

        assert_eq!(
            current_draft_author_from_path(
                &agent_dir,
                Some(&PathBuf::from("agent-runs/round-072-codex-review.md")),
            ),
            None
        );
        assert_eq!(
            current_draft_author_from_path(
                &agent_dir,
                Some(&PathBuf::from("agent-runs/not-an-artifact.md")),
            ),
            None
        );
    }

    #[test]
    fn agent_attempt_output_path_preserves_existing_artifacts_append_only() {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("maestro-agent-attempt-path-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("round-003-codex-review.md"), "old").unwrap();

        let next = agent_attempt_output_path(&dir, 3, "codex", "review");

        assert_eq!(
            next.file_name().and_then(|name| name.to_str()),
            Some("round-003-codex-review-attempt-002.md")
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn operational_only_review_round_excludes_editorial_blockers() {
        let round = vec![
            review_result("Codex", "CODEX_CLI_NO_FINAL_OUTPUT", "error"),
            review_result("Gemini", "GEMINI_RIPGREP_UNAVAILABLE", "error"),
        ];

        assert!(is_operational_only_review_round(&round));
    }

    #[test]
    fn operational_only_review_round_rejects_mixed_not_ready() {
        let round = vec![
            review_result("Codex", "NOT_READY", "warn"),
            review_result("Gemini", "GEMINI_CLI_NO_FINAL_OUTPUT", "error"),
        ];

        assert!(!is_operational_only_review_round(&round));
    }

    #[test]
    fn substantive_change_ignores_whitespace_only_deltas() {
        assert!(!is_substantive_editorial_change(
            "Linha 1\r\n\r\nLinha    2",
            " Linha 1\nLinha 2 "
        ));
        assert!(is_substantive_editorial_change("Linha 1.", "Linha 1"));
    }

    #[test]
    fn quality_guard_blocks_lower_tier_shrinkage_of_stronger_text() {
        let before = "Paragrafo amplo e reflexivo. ".repeat(30);
        let after = "Resumo curto.".to_string();

        assert!(quality_guard_blocks_revision(
            Some("codex"),
            "grok",
            &before,
            &after,
            true
        ));
        assert!(!quality_guard_blocks_revision(
            Some("grok"),
            "codex",
            &before,
            &after,
            true
        ));
    }

    #[test]
    fn circular_round_order_returns_to_original_redactor_last() {
        let active = vec![
            "claude".to_string(),
            "codex".to_string(),
            "gemini".to_string(),
            "deepseek".to_string(),
            "grok".to_string(),
        ];

        let order = circular_round_turn_specs("claude", &active)
            .into_iter()
            .map(|spec| spec.key)
            .collect::<Vec<_>>();

        assert_eq!(order, vec!["codex", "gemini", "deepseek", "grok", "claude"]);
    }

    #[test]
    fn circular_resume_continues_after_latest_revision_author() {
        let dir = crate::sessions_dir()
            .join(format!(
                "maestro-circular-resume-test-{}",
                std::process::id()
            ))
            .join("agent-runs");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let artifact_path = dir.join("round-001-codex-revision.md");
        std::fs::write(
            &artifact_path,
            r#"MAESTRO_STATUS: READY
<maestro_revision_report>
{ "reviewer": "codex", "status": "READY", "custody": "revised", "changes": [] }
</maestro_revision_report>
<maestro_final_text>
Texto revisado.
</maestro_final_text>"#,
        )
        .unwrap();
        let active = vec![
            "claude".to_string(),
            "codex".to_string(),
            "gemini".to_string(),
            "deepseek".to_string(),
            "grok".to_string(),
        ];
        let specs = circular_round_turn_specs("claude", &active);
        let mut result = review_result("Codex", "READY", "ok");
        result.output_path = artifact_path.to_string_lossy().to_string();

        let progress =
            restore_circular_resume_progress(&dir, Some(&artifact_path), &[result], 1, &specs);

        assert_eq!(progress.round, 1);
        assert_eq!(progress.turn_index, 1);
        assert!(progress.valid_agents.contains("codex"));
        assert!(progress.had_substantive_change);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn serial_contract_accepts_unchanged_ready_without_final_text() {
        let stdout = r#"MAESTRO_STATUS: READY
<maestro_revision_report>
{ "reviewer": "codex", "status": "READY", "custody": "unchanged", "changes": [] }
</maestro_revision_report>"#;

        let output = validate_serial_turn_output(stdout, "READY").unwrap();

        assert!(output.final_text.is_none());
    }

    #[test]
    fn serial_contract_rejects_truncated_ready_final_text() {
        let stdout = r#"MAESTRO_STATUS: READY
<maestro_revision_report>
{ "reviewer": "codex", "status": "READY", "custody": "revised", "changes": [] }
</maestro_revision_report>
<maestro_final_text>
Texto incompleto"#;

        let error = validate_serial_turn_output(stdout, "READY").unwrap_err();

        assert!(error.contains("maestro_final_text"));
    }

    #[test]
    fn serial_contract_rejects_prompt_or_protocol_echo() {
        let stdout = r#"MAESTRO_STATUS: READY
# Maestro Editorial AI - Serial Review-Rewrite Turn
<maestro_revision_report>
{ "reviewer": "codex", "status": "READY", "custody": "unchanged", "changes": [] }
</maestro_revision_report>"#;

        let error = validate_serial_turn_output(stdout, "READY").unwrap_err();

        assert!(error.contains("prompt/protocol"));
    }

    #[test]
    fn serial_contract_rejects_duplicate_revision_report() {
        let stdout = r#"MAESTRO_STATUS: READY
<maestro_revision_report>
{ "reviewer": "codex", "status": "READY", "custody": "unchanged", "changes": [] }
</maestro_revision_report>
<maestro_revision_report>
{ "reviewer": "codex", "status": "READY", "custody": "unchanged", "changes": [] }
</maestro_revision_report>"#;

        let error = validate_serial_turn_output(stdout, "READY").unwrap_err();

        assert!(error.contains("maestro_revision_report"));
    }
}
