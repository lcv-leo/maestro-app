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
    independent_review_agent_specs, provider_cost_guard_for, sanitize_optional_positive_f64,
    sanitize_optional_positive_u64, selected_editorial_agent_specs, ReviewPanelSelectionError,
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

    let final_path: PathBuf;
    loop {
        // Operator-driven stop check at the top of every round. Granularity
        // is "between rounds" for the orchestration; in-flight CLI peer is
        // killed via `command_spawn::run_resolved_command_observed` 250ms
        // poll; in-flight API peer is dropped via `tokio::select!` in
        // `provider_retry::send_with_retry_async`.
        if cancel_token.is_cancelled() {
            let _ = write_log_record(
                log_session,
                LogEventInput {
                    level: "warn".to_string(),
                    category: "session.user.stop_completed".to_string(),
                    message: "editorial session stopped by operator between rounds".to_string(),
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
        let review_specs = match independent_review_agent_specs(
            draft_lead_key,
            &active_agent_keys,
            current_draft_author_key.as_deref(),
        ) {
            Ok(specs) => specs,
            Err(ReviewPanelSelectionError::DraftAuthorUnknown) => {
                let _ = write_log_record(
                    log_session,
                    LogEventInput {
                        level: "error".to_string(),
                        category: "session.tribunal.draft_author_unknown".to_string(),
                        message: "review cycle blocked because the current draft author could not be verified".to_string(),
                        context: Some(json!({
                            "run_id": &run_id,
                            "round": round,
                            "current_draft_path": current_draft_path.as_ref().map(|path| path.to_string_lossy().to_string()),
                            "active_agents": active_agent_keys.clone(),
                            "policy": "fail_closed_no_self_review_without_known_petitioner"
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
                    "PAUSED_DRAFT_AUTHOR_UNKNOWN",
                ));
            }
        };
        let draft_author_key = current_draft_author_key.as_deref().unwrap_or("unknown");
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
                        "draft_author_key": author_key,
                        "review_agents": review_specs.iter().map(|spec| spec.key).collect::<Vec<_>>(),
                        "policy": "agent_never_reviews_own_draft"
                    })),
                },
            );
        }
        if review_specs.is_empty() {
            let _ = write_log_record(
                log_session,
                LogEventInput {
                    level: "error".to_string(),
                    category: "session.review.no_independent_reviewer".to_string(),
                    message: "no independent review peer remains after excluding the draft author"
                        .to_string(),
                    context: Some(json!({
                        "run_id": &run_id,
                        "round": round,
                        "draft_author_key": current_draft_author_key.clone(),
                        "active_agents": active_agent_keys.clone(),
                        "policy": "select_at_least_two_agents_for_independent_editorial_consensus"
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
        let independent_reviewer_count = review_specs.len();
        let max_serial_turns = std::cmp::min(12, std::cmp::max(6, independent_reviewer_count * 3));
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

        let reviewer_index = (round.saturating_sub(1)) % review_specs.len();
        let spec = review_specs[reviewer_index];
        let previous_revision_history = build_revision_history_block(&agents);
        let review_prompt = build_serial_revision_prompt(
            request,
            &run_id,
            round,
            &current_draft,
            draft_author_key,
            spec.key,
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
                    "reviewer": spec.key,
                    "current_author": draft_author_key,
                    "stable_serial_approvals": stable_serial_approval_agents.len(),
                    "independent_reviewer_count": independent_reviewer_count,
                    "policy": "one_peer_revises_then_passes_to_next_peer_no_self_review"
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
                            "projected_review_round_cost_usd": projected_round_cost_usd,
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
        let result = run_editorial_agent_for_spec(
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
            round += 1;
            continue;
        }
        consecutive_reviewer_outage_rounds = 0;

        let artifact = read_text_file(&output_path).unwrap_or_default();
        let stdout = extract_stdout_block(&artifact).unwrap_or(artifact.as_str());
        let Some(revised_text) = extract_tagged_block(stdout, "maestro_final_text") else {
            let _ = write_log_record(
                log_session,
                LogEventInput {
                    level: "warn".to_string(),
                    category: "session.serial.contract_violation".to_string(),
                    message:
                        "serial reviewer-reviser did not return a usable maestro_final_text block"
                            .to_string(),
                    context: Some(json!({
                        "run_id": &run_id,
                        "round": round,
                        "agent": spec.key,
                        "status": result.status,
                        "policy": "strict_output_contract_required_for_serial_custody"
                    })),
                },
            );
            stable_serial_approval_agents.clear();
            round += 1;
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
                        "reviewer": spec.key,
                        "current_author": current_draft_author_key.clone(),
                        "current_chars": current_draft.chars().count(),
                        "revised_chars": revised_text.chars().count(),
                        "policy": "anti_impoverishment_quality_ratchet"
                    })),
                },
            );
            stable_serial_approval_agents.clear();
            round += 1;
            continue;
        }

        if substantive_change {
            current_draft = revised_text;
            current_draft_path = Some(output_path);
            current_draft_author_key = Some(spec.key.to_string());
            stable_serial_approval_agents.clear();
            let _ = write_log_record(
                log_session,
                LogEventInput {
                    level: "info".to_string(),
                    category: "session.serial.version_advanced".to_string(),
                    message: "serial reviewer-reviser produced a new current version".to_string(),
                    context: Some(json!({
                        "run_id": &run_id,
                        "round": round,
                        "author": spec.key,
                        "status": result.status,
                        "policy": "new_version_requires_full_independent_rotation"
                    })),
                },
            );
        } else if result.status == "READY" {
            current_draft_path = Some(output_path);
            current_draft_author_key = Some(spec.key.to_string());
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
                        "reviewer": spec.key,
                        "stable_serial_approvals": stable_serial_approval_agents.len(),
                        "stable_serial_approval_agents": stable_serial_approval_agents.iter().cloned().collect::<Vec<_>>(),
                        "required_stable_approvals": independent_reviewer_count,
                        "policy": "converge_after_full_rotation_without_substantive_change"
                    })),
                },
            );
        } else {
            stable_serial_approval_agents.clear();
        }

        if result.status == "READY"
            && !substantive_change
            && stable_serial_approval_agents.len() >= independent_reviewer_count
        {
            let path = session_dir.join("texto-final.md");
            write_text_file(&path, &strip_leading_maestro_status(&current_draft))?;
            final_path = path;
            break;
        }

        round += 1;
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
        agent_attempt_output_path, current_draft_author_from_path,
        is_operational_only_review_round, is_substantive_editorial_change,
        quality_guard_blocks_revision,
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
}
