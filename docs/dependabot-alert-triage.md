# Dependabot Alert Triage

Status: active security register.
Scope: `src-tauri/Cargo.lock`.

## Current Rust Alerts

### GHSA-wrw7-89jp-8q8g - `glib`

- Dependabot alert: `#1`.
- Severity: medium.
- Vulnerable range: `>= 0.15.0, < 0.20.0`.
- Patched version: `0.20.0`.
- Dependency path: Tauri/Wry Linux GTK/WebKit stack.
- Supported Maestro target: Windows 11+.
- Evidence:
  - `cargo tree -i glib@0.18.5 --target x86_64-pc-windows-msvc` prints no dependency path.
  - `cargo tree -i glib@0.18.5 --target all` shows the dependency through GTK/WebKit Linux crates.
  - Release workflow builds only the Windows portable executable.
- Triage decision: vulnerable code is not used by the supported Windows runtime.

### GHSA-cq8v-f236-94qc - `rand`

- Dependabot alert: `#2`.
- Severity: low.
- Vulnerable range: `>= 0.7.0, < 0.8.6`.
- Patched version: `0.8.6`.
- Dependency path: `tauri-utils -> kuchikiki -> selectors -> phf_codegen -> phf_generator -> rand@0.7.3`.
- Dependency role: build-time transitive dependency from Tauri's HTML manipulation/code generation path.
- Evidence:
  - `cargo update --dry-run` reports no compatible update for the current Tauri 2.10.3 dependency set.
  - `cargo tree -e features -i rand@0.7.3 --target x86_64-pc-windows-msvc` shows `rand@0.7.3` through build dependencies, not Maestro application code.
  - `rg "rand::|thread_rng|rng\\(|impl log::Log|set_logger|env_logger|tracing_subscriber|log::set" src-tauri src` finds no Maestro code path using `rand` or a custom logger.
  - The advisory requires a custom logger that calls `rand::rng()`/`thread_rng()` under specific reseeding and logging conditions.
- Triage decision: tolerable transitive build-time risk until Tauri's dependency graph ships a compatible patched path.

## Local Hardening Applied

- Tauri default features are disabled.
- Maestro enables only the Tauri features needed for the Windows WebView runtime:
  - `common-controls-v6`
  - `dynamic-acl`
  - `wry`
- This removes unnecessary X11 crates from the lockfile and keeps the supported build surface aligned with Windows 11+.

## Scorecard / OSV Scanner Triage

OpenSSF Scorecard SARIF can report RustSec/OSV advisories from every package recorded in `src-tauri/Cargo.lock`, including cross-platform dependencies that do not resolve for the shipped Windows target.

The current Scorecard `VulnerabilitiesID` alert maps to `cargo audit` warnings, not active cargo-audit vulnerabilities:

- `cargo audit --json`: `vulnerabilities.found=false`.
- Warning shape: 14 `unmaintained` advisories plus 1 `unsound` advisory.
- GTK/glib evidence: `cargo tree --locked --target x86_64-pc-windows-msvc -i gtk` and `cargo tree --locked --target x86_64-pc-windows-msvc -i glib` print no dependency path.
- Cross-platform evidence: `cargo tree --locked --target all -i gtk` and `cargo tree --locked --target all -i glib` show the Linux GTK/WebKit path through Tauri/Wry.

`src-tauri/osv-scanner.toml` records the current OSV exceptions with explicit reasons and `ignoreUntil = 2026-08-09`, forcing a 90-day review window:

- GTK3 / glib stack: `RUSTSEC-2024-0411`, `RUSTSEC-2024-0412`, `RUSTSEC-2024-0413`, `RUSTSEC-2024-0415`, `RUSTSEC-2024-0416`, `RUSTSEC-2024-0418`, `RUSTSEC-2024-0419`, `RUSTSEC-2024-0420`, `RUSTSEC-2024-0429`.
- GTK macro transitives: `RUSTSEC-2024-0370`.
- Tauri/urlpattern rust-unic transitives: `RUSTSEC-2025-0075`, `RUSTSEC-2025-0080`, `RUSTSEC-2025-0081`, `RUSTSEC-2025-0098`, `RUSTSEC-2025-0100`.

Do not remove these exceptions without either upgrading the upstream Tauri/Wry graph or adding a supported Linux build target and re-triaging the GTK/WebKit runtime surface.

## Follow-up Policy

- Keep Dependabot Cargo updates enabled for `/src-tauri`.
- Reopen dismissed alerts if Tauri publishes a compatible dependency path that removes the vulnerable transitive crate.
- Do not publish Linux builds until the GTK/WebKit `glib` path is upgraded or separately triaged for that target.
