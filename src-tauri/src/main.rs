#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
// Editorial spawn-funnel guard: same `deny` policy as lib.rs (Codex NB-5).
// main.rs has no editorial spawn surface today, but this prevents the bin
// target from drifting into bypassing the funnel in future changes.
#![deny(clippy::disallowed_methods)]

fn main() {
    maestro_editorial_ai_lib::run()
}
