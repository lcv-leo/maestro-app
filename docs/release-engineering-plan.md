# Release Engineering Plan

Status: planning baseline.
Target platform: Windows 11+.

## GitHub Security Features

Maestro is developed as if these GitHub features are already active:

- Secret Scanning.
- Code Scanning with GitHub CodeQL Default Setup only.
- Dependabot alerts.
- Dependabot version updates.
- Private vulnerability reporting.

Security alerts are release blockers until triaged.

## GitHub Releases

Release readiness requires:

- Clean working tree.
- Passing CI and CodeQL Default Setup.
- No secret-shaped strings in tracked files.
- No private protocol, draft, evidence cache, or transcript committed.
- Updated `CHANGELOG.md`.
- Updated README and security docs when behavior changes.
- Annotated tag.
- GitHub Release notes that identify installer status, Windows 11+ target, portable layout, checksums, and known limitations.

## GitHub Packages

GitHub Packages is planned but disabled until package identity and privacy posture are finalized.

Candidate package surfaces:

- An npm package for shared schemas or CLI helpers.
- A package containing reusable editorial protocol schemas.
- Release artifacts attached to GitHub Releases for the Windows app itself.

No package publishing workflow should be enabled until token handling and package names are explicitly approved.

## GitHub Sponsors

Sponsors support is active through `.github/FUNDING.yml`, mirroring `cross-review-mcp` with `github: lcv-leo` and the Maestro GitHub Pages URL as the custom funding link.

## GitHub Pages

GitHub Pages uses the modern GitHub Actions source, not the legacy `gh-pages` branch. The public support page lives in `site/` and is deployed by `.github/workflows/pages.yml`.

## CodeQL Mode

CodeQL must stay on GitHub Default Setup. Advanced Setup is prohibited unless the operator first approves a written technical justification.

## Pre-Public Audit

Before changing the repository from private to public:

- Run full-history secret scanning.
- Run current-tree secret scanning.
- Verify `.gitignore` excludes runtime state.
- Verify no private protocol contents, OneDrive documents, drafts, evidence caches, logs, or CLI transcripts are tracked.
- Review GitHub Actions logs for accidental disclosure.
- Review package metadata, README, screenshots, release notes, and fixtures for private data.
