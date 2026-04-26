# Maestro Editorial AI

Portable Windows editorial workbench for protocol-driven AI drafting, source verification, and multi-agent editorial convergence.

Status: planning stage.

Maestro is independent from `cross-review-mcp`; it incorporates the same strict convergence discipline in its own application logic. It is designed to run from a folder, keep runtime data out of Git, and store operator protocols, drafts, evidence, and sessions locally under ignored runtime paths.

Target platform: Windows 11+.

Planned modern stack: Tauri 2 + WebView2, React 19, Vite 8, TypeScript 6, Vitest, Biome, ESLint, and lucide-react.

## Day-Zero Security Posture

- No secrets or API keys in the repository.
- GitHub Secret Scanning, Code Scanning, CodeQL, and Dependabot are assumed active.
- Operator-supplied editorial protocols are imported through the app and stored locally, not committed.
- Public release requires pre-cloud exposure audit and full-history secret scan.

## Release Planning

GitHub Releases, GitHub Packages, and GitHub Sponsors are planned from day zero. See `docs/release-engineering-plan.md`.
