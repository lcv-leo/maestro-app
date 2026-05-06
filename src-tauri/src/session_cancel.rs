// Modulo: src-tauri/src/session_cancel.rs
// Descricao: Per-run-id cancellation token registry for editorial sessions
// shipped in v0.5.0 to support the operator-driven "Stop session" button.
//
// Rationale: a long editorial session may run for many minutes (Claude/Codex/
// Gemini CLI peers regularly take 3-7 minutes each in real operator logs).
// Pre-v0.5.0 the only way to abort was killing the entire app. v0.5.0 wires a
// `tokio_util::sync::CancellationToken` per run_id so the operator can press
// "Parar sessao" in the UI and:
//
//   - CLI peer in flight: `command_spawn::run_resolved_command_observed` polls
//     the token every 250ms and invokes `kill_process_tree` when fired (cancel
//     resolves in <500ms).
//   - API peer in flight: `provider_retry::send_with_retry_async` wraps
//     `client.send()` in `tokio::select!` against `cancel.cancelled()` so the
//     reqwest future is dropped and the connection closed (cancel resolves in
//     <2s, bounded by network round-trip).
//   - Between rounds: `run_editorial_session_core` checks `is_cancelled()` and
//     returns early with status `STOPPED_BY_USER`, leaving artifacts intact for
//     resume via `FinalizeRunningArtifactsGuard` (Drop semantics from v0.3.16).
//
// Threading model: the static `SESSION_CANCEL` map is keyed by run_id so
// concurrent sessions (rare in single-operator desktop, but possible) each get
// their own token. The `Tauri` `stop_editorial_session` command is sync (sets
// the flag immediately, returns a bool indicating whether a matching run_id
// was found) so it never blocks even if the session loop is mid-API-call.
//
// Idempotency: `signal_session_cancel` returns false for unknown run_ids
// (already finished, never started, typo) without erroring. The operator's UI
// surfaces the bool but treats both cases as "stop request acknowledged".

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use tokio_util::sync::CancellationToken;

static SESSION_CANCEL: OnceLock<Mutex<HashMap<String, CancellationToken>>> = OnceLock::new();

fn cancel_map() -> &'static Mutex<HashMap<String, CancellationToken>> {
    SESSION_CANCEL.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Register a cancellation token for the given run_id and return a clone
/// usable by the session loop. If a stale token exists for the same run_id
/// (e.g. a previous session crashed without cleanup), it is overwritten.
pub(crate) fn register_session_cancel(run_id: &str) -> CancellationToken {
    let token = CancellationToken::new();
    let mut guard = cancel_map()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    guard.insert(run_id.to_string(), token.clone());
    token
}

/// Signal cancellation for the given run_id. Returns true if a matching
/// token was found and signaled, false otherwise (idempotent: repeated calls
/// or unknown run_ids return false without erroring).
pub(crate) fn signal_session_cancel(run_id: &str) -> bool {
    let guard = cancel_map()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(token) = guard.get(run_id) {
        token.cancel();
        true
    } else {
        false
    }
}

/// Remove the cancellation token for a run_id. Should be called by the
/// session loop when the session completes (success, failure, or cancellation)
/// so the static map does not grow unbounded across many sessions.
pub(crate) fn unregister_session_cancel(run_id: &str) {
    let mut guard = cancel_map()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    guard.remove(run_id);
}

/// RAII guard ensuring the cancellation token entry is removed even if the
/// session loop panics or returns early. Constructed by the session loop
/// after `register_session_cancel`; dropped at session end.
pub(crate) struct CancelTokenGuard {
    run_id: String,
}

impl CancelTokenGuard {
    pub(crate) fn new(run_id: String) -> Self {
        Self { run_id }
    }
}

impl Drop for CancelTokenGuard {
    fn drop(&mut self) {
        unregister_session_cancel(&self.run_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signal_unknown_run_id_returns_false() {
        // Use a synthetic run_id that no real session would have to avoid
        // colliding with concurrent tests in the same process.
        let id = "test-unknown-run-id-7d8a9c2e";
        assert!(!signal_session_cancel(id));
    }

    #[test]
    fn register_then_signal_then_unregister_roundtrips() {
        let id = "test-register-roundtrip-8e9b0f1d";
        let token = register_session_cancel(id);
        assert!(!token.is_cancelled(), "fresh token must not be cancelled");
        assert!(
            signal_session_cancel(id),
            "signal must succeed for registered run_id"
        );
        assert!(token.is_cancelled(), "token must be cancelled after signal");
        unregister_session_cancel(id);
        assert!(
            !signal_session_cancel(id),
            "second signal after unregister must return false"
        );
    }

    #[test]
    fn signal_is_idempotent_after_first_cancel() {
        let id = "test-idempotent-cancel-2a3b4c5d";
        let _token = register_session_cancel(id);
        assert!(signal_session_cancel(id));
        assert!(
            signal_session_cancel(id),
            "second signal must still return true while token registered"
        );
        unregister_session_cancel(id);
    }

    #[test]
    fn cancel_token_guard_unregisters_on_drop() {
        let id = "test-guard-drop-9f0e1d2c";
        let _token = register_session_cancel(id);
        {
            let _guard = CancelTokenGuard::new(id.to_string());
            assert!(signal_session_cancel(id));
        }
        assert!(
            !signal_session_cancel(id),
            "guard Drop must unregister so subsequent signal returns false"
        );
    }

    #[test]
    fn guard_drop_runs_on_panic() {
        // Anti-regression: ensures the cancel registry stays clean even when
        // the session loop panics mid-flight.
        let id = "test-guard-panic-3a4b5c6d";
        let _token = register_session_cancel(id);
        let result = std::panic::catch_unwind(|| {
            let _guard = CancelTokenGuard::new(id.to_string());
            panic!("synthetic panic for Drop semantics test");
        });
        assert!(result.is_err());
        assert!(
            !signal_session_cancel(id),
            "guard Drop must run on panic so subsequent signal returns false"
        );
    }
}
