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

## Follow-up Policy

- Keep Dependabot Cargo updates enabled for `/src-tauri`.
- Reopen dismissed alerts if Tauri publishes a compatible dependency path that removes the vulnerable transitive crate.
- Do not publish Linux builds until the GTK/WebKit `glib` path is upgraded or separately triaged for that target.
