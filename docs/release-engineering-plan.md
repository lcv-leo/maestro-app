# Release Engineering Plan

Status: planning baseline.
Target platform: Windows 11+.

## GitHub Security Features

Maestro is developed as if these GitHub features are already active:

- Secret Scanning.
- Code Scanning with CodeQL.
- Dependabot alerts.
- Dependabot version updates.
- Private vulnerability reporting.

Security alerts are release blockers until triaged.

## GitHub Releases

Release readiness requires:

- Clean working tree.
- Passing CI and CodeQL.
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

Sponsors support is planned but inactive.

`.github/FUNDING.yml.example` exists only as a placeholder. Rename it to `.github/FUNDING.yml` after the operator confirms the GitHub Sponsors account or external funding URL.

## Pre-Public Audit

Before changing the repository from private to public:

- Run full-history secret scanning.
- Run current-tree secret scanning.
- Verify `.gitignore` excludes runtime state.
- Verify no private protocol contents, OneDrive documents, drafts, evidence caches, logs, or CLI transcripts are tracked.
- Review GitHub Actions logs for accidental disclosure.
- Review package metadata, README, screenshots, release notes, and fixtures for private data.
