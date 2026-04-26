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

Versioning convention:

- App and release labels use `vX.X.X`.
- `package.json` stores the numeric semver core, for example `0.1.0`; Git tags and changelog headings include the `v` prefix, for example `v0.1.0`.
- Every release or scaffold milestone updates `CHANGELOG.md` under a concrete `vX.X.X` heading before Commit & Sync.

Release readiness requires:

- Clean working tree.
- Passing CI and CodeQL Default Setup.
- No secret-shaped strings in tracked files.
- No private protocol, draft, evidence cache, or transcript committed.
- Updated `CHANGELOG.md`.
- Updated README and security docs when behavior changes.
- Annotated tag.
- GitHub Release notes that identify installer status, Windows 11+ target, portable layout, checksums, and known limitations.

Distribution policy:

- GitHub Releases is the primary human-facing distribution channel.
- Windows releases are ZIP archives containing the portable executable, license, README, changelog, and checksum.
- The release workflow uses `tauri build --no-bundle`; it does not create an MSI, NSIS installer, or NuGet package.
- Releases are created only from `vX.X.X` tags or an explicit manual workflow dispatch pointing to an existing `vX.X.X` tag.
- `v0.X.X` releases are marked as GitHub prereleases and are not promoted as `latest`.
- Portable archives receive GitHub artifact attestation when the release workflow runs.

## GitHub Packages

GitHub Packages is enabled through GHCR/OCI publishing in `.github/workflows/release.yml`.

Package policy:

- No NuGet package is used for Maestro's Windows app distribution.
- The package is an OCI mirror of the same Windows portable ZIP published to GitHub Releases.
- The package name is `ghcr.io/lcv-leo/maestro-app-windows-portable`.
- Human users should use GitHub Releases; GitHub Packages is for automation, provenance, and machine retrieval.
- GHCR `latest` is reserved for stable `v1.0.0+` releases.

Future package surfaces, such as npm packages for shared schemas, require a separate approval before publishing.

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
