# Changelog

All notable changes to Maestro Editorial AI will be documented in this file.

## [v0.1.0] - 2026-04-26

- Initial architecture planning for a portable Windows editorial workbench.
- Added day-zero repository hygiene, CodeQL, CI hygiene checks, Dependabot scaffold, and private protocol ignore policy.
- Added GitHub release engineering plan, governance docs, generated release-note configuration, and Windows 11+ modern stack baseline.
- Added minimal TypeScript project marker so CodeQL has a source tree to analyze before the app scaffold lands.
- Switched CodeQL to GitHub Default Setup only, added modern GitHub Pages artifact deployment, and enabled GitHub Sponsors funding metadata.
- Added the first Tauri/React scaffold and structured NDJSON diagnostic logging under ignored `data/logs/`.
- Added the first background-orchestration UI concept with friendly progress indicators and selectable interface verbosity.
- Added editorial session workflow documentation for prompt intake, full protocol reading, trilateral unanimity, final Markdown, and session minutes.
- Added MainSite compatibility and Cloudflare D1 import/export planning for `bigdata_db.mainsite_posts`.
- Added the integrated editor decision: Maestro uses PostEditor parity over generic TipTap so functionality and final HTML match the existing MainSite editor.
- Added a Maestro-local PostEditor parity module copied from the current `admin-app/MainSite/PostEditor` support surface.
- Added Web Evidence Engine planning for fetch, curl-compatible replay, web search, rendered collection, and human-assisted browser capture for CAPTCHA/login/consent workflows.
- Added ABNT Citation Engine planning and defined Maestro as a deterministic fourth peer that can block final delivery independently of the three AI agents.
- Added Link Integrity Engine planning for link extraction, checking, sanitization, correction proposals, and cross-review escalation.
- Reinforced the inviolable unanimity rule: no final text is delivered while any peer divergence remains, regardless of time, cost, or round count.
- Added Runtime Bootstrapper planning for first-run dependency checks, authorized background installation/update, CLI configuration, and authentication flows inside Maestro UI.
- Added Cloudflare credential settings planning with account ID/API token fields, permission validation, API-first D1 operations, and Wrangler fallback.
- Added official AI provider API/SDK credential planning so Claude, Codex/OpenAI, and Gemini can run through CLI, API, or hybrid transports.
- Added operator-selectable credential storage planning for encrypted vault, Windows user environment variables, or warned local JSON config.
- Established the project versioning and changelog convention as `vX.X.X`.
- Required every Wrangler fallback invocation to use `wrangler@latest`, with automatic update authorization when the fallback CLI path is needed.
- Added a CLI Agent Audit for Codex, Claude, and Gemini, including official documentation links, local headless smoke results, structured-output risks, and adapter hard gates.
- Enabled GitHub Pages first-run provisioning through `configure-pages` and Dependabot Cargo updates now that the Tauri manifest exists.
- Added a tag-driven release workflow that builds a portable Windows ZIP, publishes GitHub Releases, mirrors the release archive to GitHub Packages through GHCR/OCI, and avoids NuGet/installer distribution.
- Triaged the initial Rust Dependabot alerts, constrained Tauri features for the Windows 11+ target, removed unnecessary X11 dependencies from the lockfile, and documented the remaining transitive Tauri risk register.
- Added the three-mode configuration persistence contract: JSON local, Windows env-var hybrid, and Cloudflare remote persistence through D1 `maestro_db` plus Cloudflare Secrets Store.
- Updated the release artifact upload/download actions to Node 24-capable versions to remove GitHub Actions Node 20 deprecation warnings.
