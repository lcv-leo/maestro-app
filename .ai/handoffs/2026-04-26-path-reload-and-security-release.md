# Handoff - 2026-04-26 - PATH Reload, Security Alerts, Releases, Packages

Use this handoff after restarting the terminal/session to reload the Rust `PATH`.

Resume phrase:

```text
Retome o maestro-app pelo handoff .ai/handoffs/2026-04-26-path-reload-and-security-release.md
```

## Context

- Repository: `C:\Users\leona\lcv-workspace\maestro-app`
- Branch: `main`
- Remote: `https://github.com/lcv-leo/maestro-app.git`
- Cross-review session: `7511f0cf-7f79-40e7-83c5-bd7c14f31f67`
- Cross-review outcome: converged, caller READY plus Claude READY plus Gemini READY in round 3.

## PATH Note

Rust is installed at:

```text
C:\Users\leona\.cargo\bin
```

In this session, `cargo check` passed through the absolute path, and `npm run tauri -- build --ci --no-bundle` passed after temporarily prepending `%USERPROFILE%\.cargo\bin` to `PATH`. Restarting the terminal/session should reload the persistent user PATH.

Quick verification after restart:

```powershell
cargo --version
rustc --version
npm run tauri -- build --ci --no-bundle
```

## Completed Work

- Added `.github/workflows/release.yml`.
- GitHub Releases are the human-facing distribution channel.
- Release workflow builds a portable Windows 11+ ZIP with the Tauri executable, README, LICENSE, CHANGELOG, `PORTABLE-RUN.txt`, and `SHA256SUMS.txt`.
- GitHub Packages is enabled through a GHCR/OCI mirror: `ghcr.io/lcv-leo/maestro-app-windows-portable`.
- NuGet is intentionally not used.
- `v0.X.X` releases are marked prerelease and not promoted as `latest`.
- Release workflow has per-tag concurrency and explicit release asset checks.
- Added `docs/dependabot-alert-triage.md`.
- Added `docs/configuration-persistence.md`.
- Updated the settings UI with three persistence modes:
  - JSON local: all configuration and secrets in ignored JSON.
  - Windows env var hybrid: only tokens/API keys in user env vars; non-secret config remains JSON.
  - Cloudflare remote: config in D1 `maestro_db`; raw secrets in Cloudflare Secrets Store; D1 stores only secret references.
- Updated README, CHANGELOG, architecture plan, Cloudflare credential contract, AI provider credential contract, and `.gitignore`.
- Tauri features are constrained for the Windows target and unnecessary X11 dependencies were removed from `Cargo.lock`.

## Important Cloudflare Constraint

Cloudflare Secrets Store values are not read back in plaintext after storage. In Cloudflare persistence mode, Maestro should read metadata/status and secret references, not raw secret values. Local desktop adapters that need raw API keys must either receive a fresh operator-provided value for that session or route through a Cloudflare-side broker that can consume Secrets Store values.

Official references used:

- `https://developers.cloudflare.com/d1/`
- `https://developers.cloudflare.com/api/operations/cloudflare-d1-create-database`
- `https://developers.cloudflare.com/secrets-store/`
- `https://developers.cloudflare.com/api/resources/secrets_store/`
- `https://developers.cloudflare.com/secrets-store/manage-secrets/`

## Validations Run

- `actionlint` over all workflows: pass.
- `npm run typecheck`: pass.
- `npm run build`: pass, with only the expected Vite large chunk warning.
- `C:\Users\leona\.cargo\bin\cargo.exe check --target x86_64-pc-windows-msvc`: pass.
- `npm audit --audit-level=moderate`: pass, 0 vulnerabilities.
- `npm run tauri -- build --ci --no-bundle` with `.cargo\bin` temporarily prepended to `PATH`: pass.
- Secret-shaped `rg` scan: no matches.
- `git diff --check`: pass.
- `git check-ignore -v release/test.zip docs/source-protocols/protocolo-editorial-v1-10-0.md`: both ignored.

## Dependabot Alerts

- `glib` alert `#1`, `GHSA-wrw7-89jp-8q8g`: triaged as not used by the supported Windows target.
- `rand` alert `#2`, `GHSA-cq8v-f236-94qc`: triaged as tolerable build-time transitive risk through Tauri until upstream publishes a compatible path.

After sync, verify:

```powershell
gh api repos/lcv-leo/maestro-app/dependabot/alerts?state=open --jq '.[] | {number, package: .dependency.package.name, state, dismissed_reason}'
gh run list --repo lcv-leo/maestro-app --limit 10
```
