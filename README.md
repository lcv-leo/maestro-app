# Maestro Editorial AI

Portable Windows editorial workbench for protocol-driven AI drafting, source verification, and multi-agent editorial convergence.

[![CI](https://github.com/lcv-leo/maestro-app/actions/workflows/ci.yml/badge.svg)](https://github.com/lcv-leo/maestro-app/actions/workflows/ci.yml)
[![Pages](https://github.com/lcv-leo/maestro-app/actions/workflows/pages.yml/badge.svg)](https://github.com/lcv-leo/maestro-app/actions/workflows/pages.yml)
![CodeQL](https://img.shields.io/badge/CodeQL-default%20setup-enabled-brightgreen)
![status](https://img.shields.io/badge/status-planning-yellow)
![target](https://img.shields.io/badge/target-Windows%2011%2B-blue)
![stack](https://img.shields.io/badge/stack-Tauri%202%20%2B%20React%2019-blueviolet)
![runtime](https://img.shields.io/badge/runtime-portable-green)
![state](https://img.shields.io/badge/state-JSON%2FNDJSON-informational)
![license](https://img.shields.io/badge/license-AGPL--3.0-blue)

Status: planning stage.

Maestro is independent from `cross-review-mcp`; it incorporates the same strict convergence discipline in its own application logic. It is designed to run from a folder, keep runtime data out of Git, and store operator protocols, drafts, evidence, and sessions locally under ignored runtime paths.

Target platform: Windows 11+.

Planned modern stack: Tauri 2 + WebView2, React 19, Vite 8, TypeScript 6, Vitest, Biome, ESLint, and lucide-react.

## Day-Zero Security Posture

- No secrets or API keys in the repository.
- GitHub Secret Scanning, Code Scanning, CodeQL, and Dependabot are assumed active.
- CodeQL uses GitHub Default Setup. Advanced Setup requires prior justification and explicit operator authorization.
- Operator-supplied editorial protocols are imported through the app and stored locally, not committed.
- Public release requires pre-cloud exposure audit and full-history secret scan.

## Release Planning

GitHub Releases, GitHub Packages, GitHub Pages, and GitHub Sponsors are planned from day zero. See `docs/release-engineering-plan.md`.
