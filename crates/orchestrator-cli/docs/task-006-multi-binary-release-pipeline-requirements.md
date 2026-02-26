# TASK-006 Requirements: Multi-Binary Release Pipeline

## Phase
- Workflow phase: `requirements`
- Workflow ID: `1ab4c121-070a-4847-b117-df5fed7ddcbe`
- Task: `TASK-006`
- Linked requirement: `REQ-006`

## Objective
Define a deterministic, repository-safe release pipeline that builds and
packages the AO runtime binary set:
- `ao`
- `agent-runner`
- `llm-cli-wrapper`
- `llm-mcp-server`

The pipeline must produce reproducible artifact naming, supported-platform
matrix documentation, and checksum verification instructions that operators can
use in CI and scripted release flows.

## Existing Baseline Audit

| Capability area | Current location | Current state | Gap |
| --- | --- | --- | --- |
| Release workflow entry point | `.github/workflows/release.yml` | Release workflow exists and runs on `v*` tags and `version/**` branches | Contract not documented in task-scoped requirements artifact |
| Runtime binary build set | `.github/workflows/release.yml`, `.cargo/config.toml` | Build command includes all four required packages in one run | No explicit acceptance contract for matrix + binary list drift detection |
| Artifact packaging format | `.github/workflows/release.yml` | Stage directory + archive packaging is implemented per target | Deterministic naming/layout contract not written down for operators |
| Version traceability | `.github/workflows/release.yml` | Version derives from tag name or `<sanitized-branch>-<sha7>` | No formal requirement language tied to release artifact names |
| Checksum output | `.github/workflows/release.yml` | `SHA256SUMS.txt` generated from release archives | Verification procedure not documented in TASK-006 artifacts |
| Dry-run release path | `.github/workflows/release.yml`, `.cargo/config.toml` | Preview builds happen on `version/**`; local release build alias exists | End-to-end dry-run expectations are not explicitly scoped |

## Artifact Contract

### Required binary set per target
- Unix targets:
  - `ao`
  - `agent-runner`
  - `llm-cli-wrapper`
  - `llm-mcp-server`
- Windows targets:
  - `ao.exe`
  - `agent-runner.exe`
  - `llm-cli-wrapper.exe`
  - `llm-mcp-server.exe`

### Supported target matrix
| Runner OS | Rust target triple | Archive extension |
| --- | --- | --- |
| `ubuntu-latest` | `x86_64-unknown-linux-gnu` | `.tar.gz` |
| `macos-15-intel` | `x86_64-apple-darwin` | `.tar.gz` |
| `macos-14` | `aarch64-apple-darwin` | `.tar.gz` |
| `windows-latest` | `x86_64-pc-windows-msvc` | `.zip` |

### Naming and directory layout
- Archive base name format:
  - `ao-<version>-<target>`
- Version derivation:
  - tag builds: `<version> = <tag name>` (for example `v0.2.0`)
  - preview builds: `<version> = <sanitized-branch>-<sha7>`
- Archive content root directory must match archive base name exactly.
- Archive content must contain only the required runtime binaries for that
  target (plus default archive container metadata, if any).
- Publish-stage output must include `SHA256SUMS.txt` covering all produced
  `.tar.gz` and `.zip` files in stable sorted order.

## Scope
In scope for implementation after this requirements phase:
- Keep one automated release build path that produces all four required
  binaries in a single run per matrix target.
- Keep artifact matrix, naming, and archive layout deterministic.
- Keep release outputs organized for scripted consumption (`dist/` artifact tree
  and checksums file).
- Document checksum generation and verification procedure for operators.
- Preserve non-publishing dry-run behavior for preview release validation.

Out of scope for this task:
- Adding new runtime binaries outside the four-binary AO set.
- Expanding platform targets beyond the defined matrix.
- Signing, notarization, SBOM, provenance attestations, or package-manager
  distribution channels.
- Runtime behavior/performance guarantees unrelated to packaging.
- Manual edits to `.ao` state files.

## Constraints
- Repository-safe and deterministic behavior only.
- Rust-only workspace constraints remain unchanged.
- Build and package steps must remain non-interactive and CI-friendly.
- Release publication must remain tag-gated (`refs/tags/v*`).
- Checksum generation must be deterministic (stable sorted archive list).
- Docs must describe exactly what the release workflow emits today.

## Acceptance Criteria
- `AC-01`: Release documentation defines artifact naming convention, supported
  target platforms, and checksum/verification procedure.
- `AC-02`: An automated release build path produces all four required binaries
  in one run for each matrix target.
- `AC-03`: Each produced artifact includes version metadata traceable to a git
  tag or commit-derived preview identifier.
- `AC-04`: Release outputs are organized in deterministic directory/filename
  structure suitable for scripted consumption.
- `AC-05`: A dry-run release path can be executed locally or in CI without
  publishing a GitHub Release.
- `AC-06`: Archive contents are verifiable against the expected per-target
  binary list.
- `AC-07`: Checksum output is generated for all produced archives and can be
  validated by operators.

## Verification Matrix

| Requirement | Verification method |
| --- | --- |
| `AC-01` | Documentation review of README + task requirements artifact |
| `AC-02` | Release workflow review: matrix build command includes all 4 packages |
| `AC-03` | Version-step review and artifact name inspection in workflow output |
| `AC-04` | Archive name/layout inspection (`ao-<version>-<target>`) |
| `AC-05` | Preview branch run (`version/**`) and local dry-run build command |
| `AC-06` | Archive listing check (`tar -tzf` / `unzip -l`) for required binaries |
| `AC-07` | `SHA256SUMS.txt` generation + checksum verification command |

## Deterministic Deliverables for Implementation Phase
- Release pipeline contract alignment in `.github/workflows/release.yml` where
  behavior drifts from this requirements document.
- Operator-facing release documentation updates in `README.md`.
- TASK-006 implementation notes documenting concrete validation commands and
  evidence expectations for release artifacts.
