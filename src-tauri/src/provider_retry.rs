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

use crate::logging::{write_log_record, LogEventInput, LogSession};
use crate::sanitize_text;

/// Build a `reqwest::blocking::Client` with the Maestro user-agent and an
/// optional per-request timeout. `None` means rely on the OS-level connect
/// timeout (~30s on Windows). Used by every editorial API runner.
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

/// Send a provider HTTP request with bounded retry on transient network
/// errors (1 retry, 1.5s backoff) and on HTTP 429 responses (up to 2
/// retries respecting `Retry-After` header, capped at 120s). Non-transient
/// errors and non-429 HTTP responses are returned unchanged.
pub(crate) fn send_with_retry<F>(
    log_session: &LogSession,
    run_id: &str,
    provider_label: &str,
    mut make_request: F,
) -> Result<reqwest::blocking::Response, reqwest::Error>
where
    F: FnMut() -> Result<reqwest::blocking::Response, reqwest::Error>,
{
    let mut attempt = 1u32;
    loop {
        let result = make_request();
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
                    std::thread::sleep(Duration::from_secs(retry_after));
                    attempt += 1;
                    continue;
                }
                return Ok(response);
            }
            Err(error) if attempt < PROVIDER_RETRY_MAX_ATTEMPTS => {
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
                std::thread::sleep(Duration::from_millis(PROVIDER_RETRY_NETWORK_BACKOFF_MS));
                attempt += 1;
                continue;
            }
            Err(error) => return Err(error),
        }
    }
}
