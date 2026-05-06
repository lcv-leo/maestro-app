//! Provider HTTP networking primitives shared by every editorial API peer.
//!
//! Extracted from `lib.rs` in the v0.3.20 split per `docs/code-split-plan.md`
//! migration order step 3 ("AI provider credentials/probes"). Behavior
//! preserved: retry policy, Retry-After parsing, and `build_api_client`
//! defaults are identical to the pre-extraction inline definitions.
//!
//! The 4 provider runner functions themselves (`run_deepseek_api_agent`,
//! `run_openai_api_agent`, `run_anthropic_api_agent`, `run_gemini_api_agent`)
//! stay in `lib.rs` for this batch and will move in v0.3.21 along with the
//! provider-specific request body shapes and response parsers.

use chrono::{DateTime, Utc};
use reqwest::blocking::Client;
use serde_json::json;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

use crate::logging::{write_log_record, LogEventInput, LogSession};
use crate::sanitize_text;

/// Outcome of an async provider HTTP request, distinguishing operator
/// cancellation from genuine network errors. v0.5.0+ runners must surface
/// `Cancelled` as `STOPPED_BY_USER` artifact status; `Network(e)` keeps the
/// existing `PROVIDER_NETWORK_ERROR` semantics.
#[derive(Debug)]
pub(crate) enum ProviderRequestOutcome {
    Cancelled,
    Network(reqwest::Error),
}

/// Build a `reqwest::blocking::Client` with the Maestro user-agent and an
/// optional per-request timeout. `None` means rely on the OS-level connect
/// timeout (~30s on Windows). Used by sync probe paths.
pub(crate) fn build_api_client(timeout: Option<Duration>) -> Result<Client, reqwest::Error> {
    let mut client_builder = Client::builder().user_agent(format!(
        "Maestro Editorial AI/{}",
        env!("CARGO_PKG_VERSION")
    ));
    if let Some(timeout) = timeout {
        client_builder = client_builder.timeout(timeout);
    }
    client_builder.build()
}

/// Async equivalent of `build_api_client`: returns a `reqwest::Client`
/// (async) with the same user-agent and optional timeout. Used by every
/// editorial API runner since v0.5.0 so an in-flight HTTP request can be
/// dropped via `tokio::select!` against a cancellation token.
pub(crate) fn build_api_client_async(
    timeout: Option<Duration>,
) -> Result<reqwest::Client, reqwest::Error> {
    let mut client_builder = reqwest::Client::builder().user_agent(format!(
        "Maestro Editorial AI/{}",
        env!("CARGO_PKG_VERSION")
    ));
    if let Some(timeout) = timeout {
        client_builder = client_builder.timeout(timeout);
    }
    client_builder.build()
}

/// Maximum attempts (initial + retries) for the `send_with_retry` policy.
/// `2` means at most one retry on transient network errors and at most one
/// Retry-After-respecting wait on HTTP 429.
pub(crate) const PROVIDER_RETRY_MAX_ATTEMPTS: u32 = 2;

/// Backoff between attempts when the request errored at the network layer
/// (DNS, connect, read timeout). Kept short so total per-spawn overhead is
/// bounded at ~1.5s instead of the full request timeout.
pub(crate) const PROVIDER_RETRY_NETWORK_BACKOFF_MS: u64 = 1500;

/// Default sleep when a 429 response carries no `Retry-After` header.
pub(crate) const PROVIDER_RETRY_429_DEFAULT_SECS: u64 = 30;

/// Hard cap for any `Retry-After` value to prevent providers from holding
/// the session loop hostage with multi-minute waits.
pub(crate) const PROVIDER_RETRY_429_CAP_SECS: u64 = 120;

/// Parse the Retry-After header per RFC 7231: either a delta-seconds integer
/// or an HTTP-date. Returns `None` if absent or unparseable.
pub(crate) fn parse_retry_after_header(headers: &reqwest::header::HeaderMap) -> Option<u64> {
    let value = headers.get(reqwest::header::RETRY_AFTER)?.to_str().ok()?;
    if let Ok(seconds) = value.trim().parse::<u64>() {
        return Some(seconds);
    }
    if let Ok(date) = DateTime::parse_from_rfc2822(value.trim()) {
        let now = Utc::now();
        let delta = date.with_timezone(&Utc) - now;
        return Some(delta.num_seconds().max(0) as u64);
    }
    None
}

// `send_with_retry` (sync) was removed in v0.5.0 along with the migration
// to `send_with_retry_async`. The 4 provider runners and DeepSeek now go
// through the async path so an in-flight HTTP request honors the operator's
// cancellation token. The blocking `Client` is still built via
// `build_api_client` for the short-lived `/models` resolve probe inside
// each runner.

/// Async send-with-retry that races the request future against a
/// `CancellationToken` so an operator-driven "Stop session" press aborts
/// the in-flight HTTP request in <2s. Same retry policy as the sync
/// `send_with_retry` (1 network retry + 1 Retry-After-respecting 429
/// retry, capped at 120s). Returns `Cancelled` as soon as the token
/// fires; otherwise mirrors the sync function.
pub(crate) async fn send_with_retry_async(
    log_session: &LogSession,
    run_id: &str,
    provider_label: &str,
    cancel_token: &CancellationToken,
    initial_builder: reqwest::RequestBuilder,
) -> Result<reqwest::Response, ProviderRequestOutcome> {
    let mut attempt = 1u32;
    let mut next_builder = Some(initial_builder);
    loop {
        let attempt_builder = match next_builder.take() {
            Some(builder) => builder,
            // RequestBuilder::try_clone returns None if body is a stream;
            // every editorial runner uses JSON bodies (cloneable bytes), so
            // this branch is reachable only as a defensive fallback.
            None => unreachable!("retry attempted after non-cloneable request builder"),
        };
        let retry_clone = attempt_builder.try_clone();
        let response_fut = attempt_builder.send();
        let result = tokio::select! {
            biased;
            _ = cancel_token.cancelled() => return Err(ProviderRequestOutcome::Cancelled),
            r = response_fut => r,
        };
        match result {
            Ok(response) => {
                if response.status().as_u16() == 429 && attempt < PROVIDER_RETRY_MAX_ATTEMPTS {
                    let retry_after = parse_retry_after_header(response.headers())
                        .unwrap_or(PROVIDER_RETRY_429_DEFAULT_SECS)
                        .min(PROVIDER_RETRY_429_CAP_SECS);
                    let _ = write_log_record(
                        log_session,
                        LogEventInput {
                            level: "warn".to_string(),
                            category: "session.provider.retry_after_429".to_string(),
                            message: "provider returned HTTP 429; sleeping until retry".to_string(),
                            context: Some(json!({
                                "run_id": run_id,
                                "provider": provider_label,
                                "attempt": attempt,
                                "retry_after_seconds": retry_after,
                                "retry_after_source": parse_retry_after_header(response.headers())
                                    .map(|_| "header")
                                    .unwrap_or("default"),
                            })),
                        },
                    );
                    if let Some(next) = retry_clone {
                        // Cancel-aware sleep: aborts the wait if operator presses stop.
                        tokio::select! {
                            biased;
                            _ = cancel_token.cancelled() => {
                                return Err(ProviderRequestOutcome::Cancelled);
                            }
                            _ = tokio::time::sleep(Duration::from_secs(retry_after)) => {}
                        }
                        attempt += 1;
                        next_builder = Some(next);
                        continue;
                    }
                }
                return Ok(response);
            }
            Err(error) if attempt < PROVIDER_RETRY_MAX_ATTEMPTS => {
                if let Some(next) = retry_clone {
                    let _ = write_log_record(
                        log_session,
                        LogEventInput {
                            level: "warn".to_string(),
                            category: "session.provider.retry_network".to_string(),
                            message: "provider network error; retrying after backoff".to_string(),
                            context: Some(json!({
                                "run_id": run_id,
                                "provider": provider_label,
                                "attempt": attempt,
                                "backoff_ms": PROVIDER_RETRY_NETWORK_BACKOFF_MS,
                                "error": sanitize_text(&error.to_string(), 240),
                                "error_is_timeout": error.is_timeout(),
                                "error_is_connect": error.is_connect(),
                            })),
                        },
                    );
                    tokio::select! {
                        biased;
                        _ = cancel_token.cancelled() => {
                            return Err(ProviderRequestOutcome::Cancelled);
                        }
                        _ = tokio::time::sleep(Duration::from_millis(
                            PROVIDER_RETRY_NETWORK_BACKOFF_MS,
                        )) => {}
                    }
                    attempt += 1;
                    next_builder = Some(next);
                    continue;
                }
                return Err(ProviderRequestOutcome::Network(error));
            }
            Err(error) => return Err(ProviderRequestOutcome::Network(error)),
        }
    }
}
