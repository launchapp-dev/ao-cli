# TASK-006 Implementation Notes: Multi-Binary Release Pipeline

## Phase Context
- Workflow phase: `requirements`
- Workflow ID: `9989748a-8d2d-4aae-845a-f1cd977cf644`
- Task: `TASK-006`
- Linked requirement: `REQ-006`

## Purpose
Translate TASK-006 requirements into a concrete implementation plan that keeps
release outputs deterministic for:
- `ao`
- `agent-runner`
- `llm-cli-wrapper`
- `llm-mcp-server`

## Non-Negotiable Constraints
- Keep the release pipeline repository-safe and non-interactive.
- Keep behavior deterministic across repeated runs for identical inputs.
- Keep release publication tag-gated (`refs/tags/v*`) only.
- Keep the platform matrix and runtime binary set explicitly defined.
- Keep per-archive metadata manifest schema stable (`ao.release.v1`).
- Keep `.ao` state mutations command-driven; do not manually edit `.ao` files.

## Proposed Change Surface

### 1) Release workflow contract alignment
Primary file:
- `.github/workflows/release.yml`

Implementation targets:
- Keep one build command that compiles the four runtime packages in one run.
- Keep matrix entries for:
  - `x86_64-unknown-linux-gnu`
  - `x86_64-apple-darwin`
  - `aarch64-apple-darwin`
  - `x86_64-pc-windows-msvc`
- Keep package steps that emit archives named
  `ao-<version>-<target>.<ext>`.
- Keep package steps that emit `release-metadata.json` inside each archive root
  directory with traceability fields (`schema`, `version`, `target`, `git_ref`,
  `git_sha`, `event_name`, `dry_run_note`, `binaries`, `files`).
- Keep publish step generation of `dist/SHA256SUMS.txt` using stable sorted
  archive input list.

### 2) Operator documentation alignment
Primary file:
- `README.md`

Implementation targets:
- Document artifact matrix and archive extension per target.
- Document version derivation contract:
  - tag build -> tag name
  - preview build -> `<sanitized-branch>-<sha7>`
- Document required `release-metadata.json` contract.
- Document checksum verification flow for downloaded artifacts.
- Cross-reference TASK-006 requirements/implementation-note artifacts for future
  maintenance.

### 3) Local dry-run path clarity
Primary file:
- `.cargo/config.toml`

Implementation targets:
- Retain `ao-bin-build-release` as the local non-publish build path for the
  four-binary runtime set.
- Ensure docs describe when to use local dry-run vs CI preview branch run.

## Expected Artifact Contract (Implementation Checkpoint)
- Unix archive:
  - `ao-<version>-<target>.tar.gz`
  - contains directory `ao-<version>-<target>/` with:
    - `ao`
    - `agent-runner`
    - `llm-cli-wrapper`
    - `llm-mcp-server`
    - `release-metadata.json`
- Windows archive:
  - `ao-<version>-<target>.zip`
  - contains directory `ao-<version>-<target>/` with:
    - `ao.exe`
    - `agent-runner.exe`
    - `llm-cli-wrapper.exe`
    - `llm-mcp-server.exe`
    - `release-metadata.json`
- `release-metadata.json` contract:
  - `schema = "ao.release.v1"`
  - `version` and `target` match archive filename segments
  - `binaries` list remains `ao`, `agent-runner`, `llm-cli-wrapper`,
    `llm-mcp-server`
  - `files` list aligns to platform extensions (`.exe` on Windows only)

## Implementation Sequence
1. Confirm release matrix and binary packaging list in
   `.github/workflows/release.yml` matches TASK-006 requirements.
2. Update README release section with matrix, naming contract, and checksum
   verification commands, including metadata manifest expectations.
3. Run local dry-run build command for compile-level validation:
   - `cargo ao-bin-build-release`
4. For CI preview validation, run release workflow from a `version/**` branch
   and inspect produced archives + metadata manifests.
5. For tag validation, verify publish job attaches all archives plus
   `SHA256SUMS.txt` to GitHub Release.

## Validation Targets
- Local compile validation:
  - `cargo ao-bin-check`
  - `cargo ao-bin-build-release`
- Archive content spot checks:
  - `tar -tzf <archive>.tar.gz`
  - `unzip -l <archive>.zip`
- Metadata manifest spot checks:
  - `tar -tzf <archive>.tar.gz | rg 'release-metadata.json'`
  - `unzip -l <archive>.zip | rg 'release-metadata.json'`
- Checksum verification:
  - `sha256sum -c SHA256SUMS.txt`

## Risks and Mitigations
- Risk: binary list drift between build and packaging steps.
  - Mitigation: keep binary list explicit and identical across workflow steps.
- Risk: matrix drift breaks expected target support.
  - Mitigation: document matrix contract and review on release workflow edits.
- Risk: preview runs accidentally publish releases.
  - Mitigation: preserve publish guard `startsWith(github.ref, 'refs/tags/v')`.
- Risk: metadata schema drift breaks downstream automation.
  - Mitigation: keep metadata field list explicit in requirements/docs and
    validate `release-metadata.json` on preview/tag runs.
