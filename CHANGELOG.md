# Changelog

All notable changes to this project will be documented in this file.

## [Unreleased]

### Fixed

- **`fix(web)`: drive transport protocol lifecycle (`transport/start` +
  `transport/shutdown`) per spec.** `animus web serve` / `animus web open`
  only called `initialize` and (optionally) `transport/info` on installed
  `transport_backend` plugins, then waited in the foreground. The
  animus-protocol v0.1.13 spec (§13) requires the host to issue
  `transport/start` with a `TransportConfig` after `initialize` so the
  plugin can bind its listener, and `transport/shutdown` before process
  exit so it can drain in-flight requests. The ops_web path now drives
  the full lifecycle: after `initialize` it sends `transport/start` with
  the control socket path + project root, and the new
  `PluginHost::shutdown_transport()` helper issues `transport/shutdown`
  (with a bounded 5 s timeout) before the generic `shutdown` RPC. Legacy
  plugins that pre-date the lifecycle (current launchapp-dev
  `animus-transport-http`, `animus-transport-graphql`, and
  `animus-web-ui` bind inside `initialize`) respond with
  METHOD_NOT_FOUND / METHOD_NOT_SUPPORTED and are handled gracefully
  with a deprecation warning so the v0.4.x web surface keeps working
  while the ecosystem upgrades. New plugin-host tests in
  `tests/concurrency.rs` assert the spec ordering
  (`initialize` → work → `transport/shutdown` → `shutdown`) and the
  legacy METHOD_NOT_FOUND swallow.

## [0.4.13] - 2026-05-27

Operational hardening of the v0.4.12 plugin extraction. Several pieces of
infrastructure shipped in v0.4.12 (signature verifier, process quota,
durability fsync, doctor diagnostics) but were not actually invoked on
the user path; v0.4.13 wires them up. Plus the v0.4.13 W-bundle ships
plugin lockfile, audit log, subprocess workflow_events back-channel,
runtime quotas, and an onboarding overhaul (`animus init`, `animus doctor`).

### Security

- **`fix(installer)`: invoke keyless cosign TrustedPublisher verifier on
  install + close cross-repo SAN attack.** The keyless cosign rewrite
  (v0.4.12 commit 518c0d9e) landed `verify_plugin_install` in
  `orchestrator-plugin-host::signature_verifier` but `animus plugin install`
  was still routing through a local `verify_with_cosign` with a weak
  `^https://github.com/<owner>/<repo>/.+` regex that would have accepted a
  bundle signed by `launchapp-dev/animus-subject-linear` against an install
  of `launchapp-dev/animus-provider-claude` (cross-repo SAN attack). Fix
  delegates to the host verifier with the TrustedPublisher regex pinned
  per-install to the specific repo segment, and re-applies the
  trusted-signers allowlist on the host verdict. 6 new tests including
  the attack-shape regression.

### Features

- **`feat(durability bundle)`: plugin lockfile + audit log + subprocess
  workflow_events back-channel + runtime quotas.** `.animus/plugins.lock`
  pins `sha256(artifact)` and `sha256(signature_bundle)`; install appends,
  upgrade refuses on mismatch without `--force`. `~/.animus/<scope>/audit.jsonl`
  is an append-only audit log of plugin installs / signatures / quota
  denials with 10 MB rotation. Subprocess phases now receive workflow events
  via a per-run Unix-domain-socket back-channel
  (`$TMPDIR/animus-event-pipes/<daemon-pid>/<subject>-<hex>.sock`) instead
  of relying on direct broadcaster handles. `RuntimeQuotas` caps trigger
  backlog (1000), subscriber memory (10 MB/sub), plugin process count (50),
  workflow concurrency (10).
- **`feat(plugin-host)`: enforce process quota via `ProcessSlotFactory` at
  spawn site.** The `PluginProcessSlot` RAII guard from the W-bundle is now
  actually acquired in `host.rs::spawn_with_options`. Wiring uses a trait
  (`ProcessSlotFactory`) the daemon installs at startup to avoid a circular
  dep between plugin-host and daemon-runtime. Spawn returns
  `ProcessSlotError::Exhausted` when at cap; the slot is released eagerly
  on shutdown.
- **`feat(durability)`: fsync session checkpoints + phase markers + task
  state writes (W3).** All atomic writes now call `File::sync_all()` +
  `fsync_rename()` so a crash mid-write cannot leave torn state.
- **`feat(daemon)`: defer `ProcessManager` broadcaster lookup to spawn time
  so subprocess workflow events actually fire.** The lookup was happening
  at construction time, before `run_daemon` installed the broadcaster,
  always returning `None`.

### Onboarding

- **`feat(onboarding)`: `animus init` interactive walkthrough + hello-world
  workflow template.** First-run experience: detects whether plugins are
  installed, offers `--auto-install`, drops a `.animus/` skeleton with a
  bundled `hello-world.yaml` workflow you can run immediately.
- **`feat(onboarding)`: `animus doctor` polish — 8 actionable diagnostic
  categories.** Replaces the previous monolithic health output with
  category-scoped checks (daemon socket, plugin install state, signature
  cache, audit log presence, etc.) each with concrete remediation steps.

### Fixes

- **`fix(web)`: partition transport plugins by `$ui/web` capability so
  `animus web open` picks the UI instead of an API transport.** Previously
  `ops_web.rs` keyed off `plugin_kind = "web_ui"`, but no installed plugin
  declares that kind — `animus-web-ui` ships as `transport_backend` like
  the API transports, so the UI bucket was empty and `transport-http` won
  the sort. The browser opened the API endpoint instead of the UI.
  Fix: `ops_web.rs` now scans each transport plugin's manifest
  `capabilities` (which `extra_capabilities` flattens into at discovery
  time) and treats any plugin advertising `$ui/web` as the UI surface.
  `animus web open` opens the UI URL when one is installed and falls back
  to the highest-priority API transport with a warning explaining how to
  remediate. `animus web serve` prints `UI:` and `API (<name>):` on their
  own labelled lines and includes `ui_url` / `api_url` / `serves_ui` in
  the JSON envelope.

  Follow-up required in `launchapp-dev/animus-web-ui` v0.1.2: declare
  `"$ui/web"` in its manifest via the v0.1.13 `extra_capabilities`
  extension point. Until that plugin ships, machines with only the
  default transports installed still open the API endpoint and get the
  warning above.
- **`fix(doctor)`: avoid nested tokio runtime in daemon RPC check.**
  `probe_daemon_health` was constructing a current-thread `Runtime` and
  calling `block_on` while already inside the `#[tokio::main]` runtime
  driving `handle_doctor`, panicking with "Cannot start a runtime from
  within a runtime" whenever the daemon control socket was present and
  `--skip-subprocess` was off. Fix converts the probe to async and
  propagates `.await` through `run_all_checks`.
- **`fix(durability)`: skip marker writes for `--phase` runs + canonicalize
  provider alias on restart resume.** Two follow-ups from codex round 8:
  the `--phase` filter path now uses `persist_phase_output_without_marker`
  so partial workflow runs do not advance the workflow marker, and
  restart-resume now normalizes `oai-runner` / `animus-oai-runner` →
  `oai` before looking up the resume target.
- **`fix(init)`: create `.animus/` before `install-defaults` + suppress
  child JSON on parent stdout.** Two codex round-9 P2s in the new
  `animus init --walkthrough`. (1) The plugin install was running before
  the template copy created `.animus/`, so `PluginLockfile::default_path`
  fell back to `~/.animus/plugins.lock`; subsequent `animus plugin lock
  list/verify` then looked at the now-existing project-local lockfile
  and missed the walkthrough's entries. Fix creates `.animus/` first so
  the lockfile location is stable. (2) `animus init --walkthrough --json`
  was passing `--json` to the child install / daemon subprocesses while
  inheriting their stdout, producing multiple JSON documents on the
  parent stdout. Fix pipes (and discards) child stdout when `json=true`,
  inherits in human mode so progress still streams.
- **`fix(workflow-runner-v2)`: unify plugin_pack_fixture tests with the
  crate-wide state serializer.** `plugin_pack_fixture_tests` had its own
  local `env_lock()` mutex while every other HOME-mutating test used
  `crate::test_env::scoped_state_serializer`. Parallel runs raced on
  `HOME` between the two groups, surfacing intermittently as
  `agent_state::tests::memory_delete_entry_by_id_removes_only_matching_entry`
  failures depending on which test got HOME first. Fix routes the 4
  plugin_pack_fixture tests through the shared serializer so a single
  mutex covers all HOME mutators. `cargo test -p workflow-runner-v2`
  (default parallel) now passes 120 tests with no flakes.
- **`fix(daemon)`: enforce default workflow concurrency from `RuntimeQuotas`
  + bind event-pipe synchronously.** Two daemon-runtime correctness
  fixes from user codex round 9: (1) `RuntimeQuotas.workflow_concurrency_max`
  (default 10) was documented but `ProcessManager::new()` only honored
  the env var; the cap was effectively unbounded for typical operators.
  `daemon_run` now installs `RuntimeQuotas::from_env()` before
  constructing `ProcessManager`, which seeds its spawn cap from the
  quota struct. `schedule_headroom(pool_size, active)` now caps the
  effective pool at `min(pool_size, runtime_quota)` so scheduler and
  trigger paths never select more work than the spawn site will accept.
  Without this, oversized `pool_size` configs would silently consume
  schedules and drain webhook events without runners ever starting.
  (2) `SubprocessEventPipe::bind` spawned an async bind task and waited
  on a `std::sync::mpsc::Receiver` from the calling thread, which could
  deadlock a current-thread runtime and stall a multi-thread worker.
  New `bind_sync` binds the socket synchronously on the calling thread
  and spawns only the reader task on the current Tokio runtime.

### Documentation

- **`docs`: refresh plugin version refs after registry bumps.** Installation,
  upgrading, web-dashboard, and CLI reference docs now match
  `plugin_registry.rs` constants. `plugin_types.rs` help text cross-references
  the registry instead of hard-coded versions to prevent future drift.

### Internal

- **`chore(cleanup)`: drop dead code, dedupe audit test, fix duplicate
  test attr, remove unused imports.** 5 → 0 workspace build warnings.
- **`chore(scripts)`: add `dispatch-wave.sh` helper for worktree-isolated
  parallel agent dispatch.** Standardizes the `git worktree add` +
  per-agent branch + merge / cleanup flow for parallel sub-agent waves.
  Not on the user path — used by the maintainer when driving multi-agent
  cleanup waves like this release.

## [0.4.12] - 2026-05-24

Closes the v0.4 arc. Finishes plugin extraction (web stack, subject adapters,
all in-tree provider mirrors, and `llm-cli-wrapper` are gone), ships the
durability items that were carrying `// TODO: v0.5` notes, and turns the
plugin host into a first-class component of daemon startup with preflight +
auto-install. Approximately 11.5K lines of code removed from this repo; the
replacement code lives in 18 standalone plugin repositories under
`launchapp-dev/`.

### Features

- **`feat(daemon preflight + auto-install)`: plugin presence check on
  startup.** New `animus daemon preflight` standalone subcommand prints the
  current installed-vs-required matrix; `animus daemon start` /
  `animus daemon run` now run the same check before booting. Default posture
  is refuse-to-start when any required role is unsatisfied — the error
  surfaces the exact `animus plugin install ...` command to remediate.
  `--auto-install` installs the daemon's recommended default plugin for any
  missing role from `launchapp-dev` releases before continuing.
  `--skip-preflight` is the escape hatch for dev iteration. JSON envelope
  is `animus.daemon.preflight.v1`.
- **`feat(plugin install-defaults)`: bulk install for the standard 5
  providers.** New `animus plugin install-defaults` installs the
  claude / codex / gemini / opencode / oai provider plugins at v0.2.1.
  Flags: `--include-oai-agent` (oai-agent v0.1.1),
  `--include-subjects` (default / requirements / linear / sqlite / markdown
  subject backends), `--include-transports` (transport-http +
  transport-graphql + web-ui). Uses the same install pipeline as
  `animus plugin install`, so signature verification and integrity checks
  apply uniformly.
- **`feat(daemon workflow events)`: workflow/events ControlClient
  subscription.** Daemon-side broadcaster now emits `phase_started` /
  `phase_completed` / `workflow_completed` / `workflow_failed`
  notifications on `workflow/events`. Subscribers can filter by
  `workflow_id` or by kinds. Closes the matching v0.5 deferral.

### Durability

- **`feat(notifications log)`: per-run JSONL.** Each run now writes
  `~/.animus/<scope>/runs/<id>/notifications.jsonl` with
  `{ seq, ts, phase, ... }` rows. 100 MB rotation; partial trailing
  lines are dropped on replay. UI clients can reconnect and replay from any
  `seq`.
- **`feat(session checkpointing + auto-resume)`: per-phase
  `<phase>.session.json` holds `{ provider, session_id, status }`. On
  daemon restart, the scheduler attempts `provider.resume_agent` through
  the plugin that originally ran the phase. Failures land as `Blocked`
  with an explicit reason rather than silently re-running.
- **`feat(idempotency markers)`: new `idempotency: idempotent |
  sideeffecting | unknown` field on phases (default `unknown`). Crash
  recovery auto-retries `idempotent` phases; everything else lands as
  `Blocked` with an actionable hint. `animus workflow resume <id>
  --force` is the manual override.
- **`feat(atomic completion markers)`: `<phase>.completed` is now written
  via tmp → fsync → rename after `persist_phase_output`. Closes the
  "output exists, daemon crashed before state transition" race.
- **`feat(plugin supervisor)`: per-plugin restart budget of 3 in 60s with
  a 5-minute `Disabled` cooldown when exhausted. Death-like errors
  auto-retry once before propagating; structured JSON-RPC errors
  propagate immediately.
- **`refactor(plugin-host)`: typed `HostError::ConnectionLost` plus
  `classify(&HostError) -> RetryDecision` API replaces the previous
  string-substring death-like classifier.

### Refactor / Deletions

- **`refactor(web)`: delete in-tree web stack
  (`orchestrator-web-server` + `orchestrator-web-api` +
  `orchestrator-web-contracts`, ~9K LOC).** `animus web serve` and
  `animus web open` now discover installed `transport_backend` + `web_ui`
  plugins, spawn them, and open the browser. The standalone
  `launchapp-dev/animus-transport-{http,graphql}` v0.2.x and
  `launchapp-dev/animus-web-ui` v0.1.0 replace the in-tree
  implementation. `animus plugin install-defaults --include-transports`
  installs the standard set.
- **`refactor(subject backends)`: delete `inproc_subject_backend.rs`
  (~1000 LOC).** All subject operations route through the
  `SubjectRouter` to an installed plugin —
  `launchapp-dev/animus-subject-default` v0.1.1 for `kind=task`,
  `launchapp-dev/animus-subject-requirements` v0.1.6 for
  `kind=requirement` (with legacy JSON state compat), plus
  `launchapp-dev/animus-subject-{linear,sqlite,markdown}` v0.1.4 for
  the other kinds. The env vars
  `ANIMUS_DAEMON_DISABLE_BUILTIN_TASK_ADAPTER` and
  `ANIMUS_DAEMON_DISABLE_BUILTIN_REQUIREMENTS_ADAPTER` are now no-ops.
  Use `ANIMUS_DAEMON_DISABLE_SUBJECT_PLUGINS=1` to skip subject
  discovery entirely.
- **`refactor(llm-cli-wrapper)`: crate deleted (~5,882 LOC across the
  multi-phase retirement).** Session DTOs, native backends, the parser,
  and the error types now live in upstream `animus-session-backend`
  v0.1.10. The `cli/launch.rs` symbols moved into
  `agent-runner::runner::launch`. `lookup_binary_in_path` was inlined
  as `which::which` at the few remaining call sites.
  `CapabilityNotSupported` moved into `orchestrator-session-host::error`.
- **`refactor(providers)`: delete the 5 in-tree provider mirrors
  (`animus-provider-{claude,codex,gemini,opencode,oai}`).** The
  standalone `launchapp-dev/animus-provider-*` v0.2.1 repos are now the
  canonical implementations and ship streaming notifications.
- **`refactor(protocol mirror)`: delete in-tree
  `animus-provider-protocol` mirror (-408 LOC).** Now consumed from
  upstream via `animus-protocol` v0.1.10.

### Protocol

- `animus-protocol` pins bumped to v0.1.10. Path through v0.1.6
  (`animus-transport-protocol` crate +
  `transport_backend_main` entrypoint) → v0.1.7 (`AgentNotification` +
  `NotificationSink` + `ProviderBackend::run_agent_streaming`) →
  v0.1.8 (subject/delete wire verb + `ControlClient` for cross-process
  control) → v0.1.9 (subscription API: `subject_watch` /
  `daemon_events` / `daemon_logs_follow`) → v0.1.10
  (`ControlClient::workflow_events`).

### Tests

- All 14 pre-existing baseline test failures resolved: 4 rewritten
  against the current surface, 8 deleted (they referenced commands
  removed in v0.4.4), 1 marked `#[ignore]` because of a pre-existing
  race that is not in this release's scope, 1 already passes after the
  surrounding refactors.
- **New: `launchapp-dev/animus-plugin-testkit` v0.1.0** — conformance
  harness with 8 baseline scenarios validated end-to-end against
  `animus-provider-claude`.

### Breaking Changes

- **`animus web serve` no longer boots an in-process axum server.**
  Requires the transport + UI plugins installed first; run
  `animus plugin install-defaults --include-transports` or
  `animus daemon preflight` for the exact remediation command.
- **Daemon requires at least one provider plugin installed at start.**
  Use `animus daemon start --auto-install` or `animus plugin
  install-defaults` first. `--skip-preflight` is the dev escape hatch.
- **`ANIMUS_DAEMON_DISABLE_BUILTIN_TASK_ADAPTER` and
  `ANIMUS_DAEMON_DISABLE_BUILTIN_REQUIREMENTS_ADAPTER` are no-ops.**
  The in-tree subject adapters are gone; install the corresponding
  subject_backend plugin (see `--include-subjects`).

### Security

- **`refactor(security): cosign keyless OIDC verification replaces the
  key-based PEM trust path.`** Every `launchapp-dev/animus-*` release
  pipeline (`animus-transport-{graphql,http}`, `animus-web-ui`, and the
  six `animus-provider-*` repos) signs through GitHub Actions OIDC +
  Sigstore Fulcio + Rekor, never against a static signing key. The
  pre-v0.4.12 verifier in `crates/orchestrator-plugin-host/src/signature_verifier.rs`
  shelled out to `cosign verify-blob --key <PEM>` against a baked
  `LAUNCHAPP_DEV_COSIGN_PUBLIC_KEY_PEM` placeholder, which could never
  match a real keyless bundle. v0.4.12 rewrites the verifier to keyless:
  trust is now anchored on the per-publisher `identity_regex` + OIDC
  issuer combination held in `TrustedPublisher`. The built-in
  `TrustedPublisher::launchapp_dev()` matches
  `^https://github\.com/launchapp-dev/[^/]+/\.github/workflows/release\.yml@refs/tags/v.*`
  against issuer `https://token.actions.githubusercontent.com`, which is
  exactly the cert SAN that GitHub Actions OIDC bakes into the
  Fulcio-issued cert for every standardized release. Manual
  verification: `cosign verify-blob --certificate-identity-regexp <regex>
  --certificate-oidc-issuer <issuer> --bundle <.bundle> <artifact>` —
  see [`docs/reference/security.md`](docs/reference/security.md#manual-cosign-verification).
- **Removed:** the baked PEM constant `LAUNCHAPP_DEV_COSIGN_PUBLIC_KEY_PEM`,
  the seed-on-first-strict-install helper `seed_launchapp_dev_trusted_key`,
  the `~/.animus/trusted-keys/` lookup (`default_trusted_keys_dir`,
  `resolve_trusted_key_for`), and the `crates/orchestrator-plugin-host/trusted-keys/launchapp-dev.pub`
  file. The `dirs` crate dependency drops out of `orchestrator-plugin-host`
  alongside. **Operators can also delete** their local
  `~/.animus/trusted-keys/` directory, the GH org
  `COSIGN_PRIVATE_KEY` / `COSIGN_PASSWORD` secrets, and any
  `~/.animus/keys/launchapp-dev.{key,pub,password}` files — none of them
  are read by v0.4.12. Sigstore Fulcio + Rekor (built into the `cosign`
  binary) is now the only trust anchor.
- **CLI flag `--trust-key` is deprecated and a no-op.** Keyless cosign
  verification has no static public-key trust anchor, so the flag is
  retained only to avoid breaking existing install scripts. Passing it
  logs a deprecation warning and proceeds via the normal keyless path.
  Removal targeted for a future release.
- **`security(plugin install)`: install-time signature policy defaults to
  `warn` for v0.4.12 only.** Now that the trust anchor is real (Fulcio +
  Rekor) rather than a placeholder PEM, the only reason to keep `warn`
  as the default for this release is the upgrade path: operators with
  pre-v0.4.12 installs whose releases predate keyless signing get a
  one-release migration window where `signature_status` is recorded but
  install does not fail closed. **v0.4.13 flips the default back to
  `strict`.** No CLI surface change. Operators with a fully v0.4.12+
  plugin set can opt in to `strict` today via
  `animus plugin install --signature-policy strict <repo>`. See
  [`docs/reference/security.md`](docs/reference/security.md#v0412-temporary-default-warn).

## [0.4.11] - 2026-05-23

Cleanup + automation hardening. Lands the v0.4.10 Node 24 sweep across the
plugin matrix and promotes the release-automation tooling from "future
work" to "shipped".

### CI

- **`ci`: merged Node 24 PRs across 15 plugin repos (deadline 2026-06-02).**
  Each plugin repo's `chore/force-node24-actions` PR (PR #1, opened in
  v0.4.10) is now landed on `main`. 14 merged clean; `animus-protocol`
  was held back because the existing `cargo fmt` job is failing on a
  pre-existing drift in `animus-plugin-runtime/src/log.rs:241` unrelated
  to the Node 24 env-var change — flagged for v0.4.12.
- **`ci(release-automation)`: add compat-test / validate-manifests /
  check-signatures scripts.** The `launchapp-dev/animus-release-automation`
  repo (shipped in v0.4.10 as v0.1.0 with `matrix.sh` + `cascade.sh`)
  now ships three additional scripts: `compat-test.sh` runs the in-tree
  `orchestrator-plugin-host` `protocol_drift` contract test against each
  plugin's pinned `animus-plugin-protocol` tag; `validate-manifests.sh`
  validates every plugin's `plugin.toml` against `animus.plugin.v1`;
  `check-signatures.sh` audits cosign bundles on each plugin's latest
  release against the `trusted-signers.yaml` defaults. README updated to
  v0.1.1 with usage + dependency notes.

## [0.4.10] - 2026-05-22

Patch release. Picks up v0.4.9 deferrals plus the broader cleanup the user has been queuing — the long-deferred `workflows_list` wire migration (HTTP only), live per-plugin `health/check` RPC fan-out, log redaction wired into every emit site, the two pre-existing flaky tests fixed at the root cause, and a `FORCE_JAVASCRIPT_ACTIONS_TO_NODE24` sweep ahead of the 2026-06-02 GitHub Actions Node 20 cutover.

### Breaking Changes

- **`http(workflows_list)`: wire shape on `/api/v1/workflows`.** The HTTP endpoint now serves `WorkflowRunSummary` rows (`id`, `definition`, `status`, `subject_id`, `started_at`, `finished_at`) instead of the full `OrchestratorWorkflow`. Daemon-internal fields (`phases`, `machine_state`, `current_phase`, `checkpoint_metadata`, decision history) no longer leak through the public HTTP API. `status` is kebab-case (`in-progress`-style) on the wire; `Escalated` collapses to `Paused` because the wire status enum lacks the variant. GraphQL `workflows` and `workflowsPaginated` still serve the full `OrchestratorWorkflow` shape, so the embedded web UI is unaffected.

### Features

- **`feat(daemon health)`: live per-plugin `health/check` fan-out.** `daemon_health()` now spawns each discovered plugin one-shot, runs the `initialize` handshake, calls `health/check`, and shuts the plugin down — all under a 3s per-plugin deadline. Probes fire concurrently via `futures_util::future::join_all`, so the wall time is roughly one probe regardless of plugin count. Failures land as `Unhealthy` rows with the error string in `last_error`; the daemon's own status stays `Healthy` because plugin-side trouble is an observability concern, not a daemon-liveness one. A long-lived plugin host pool is still v0.5 work, but `daemon health` is not hot-path enough to need it.
- **`feat(orchestrator-logging)`: redaction wired into `Logger::write_entry`.** The v0.4.9 `redact_log_entry` building block moved from `orchestrator_daemon_runtime::control::log_redact` into `orchestrator_logging::log_redact` and is invoked from `Logger::write_entry` immediately before serialization. Every emit site that goes through the logger picks up redaction automatically; the old module path remains as a re-export so external callers stay source-compatible. New regression test (`write_entry_redacts_msg_in_persisted_line`) reads the persisted `events.jsonl` line and asserts the secret never reached disk.
- **`feat(workflows_list)`: `workflows_list_summary` API method.** New `WebApiService::workflows_list_summary(query)` returns `ListPage<WorkflowRunSummary>` — the projection helper `workflow_to_run_summary` maps local `OrchestratorWorkflow` rows onto the wire shape. The HTTP handler at `/api/v1/workflows` calls this method; the existing `workflows_list` is kept for GraphQL.

### Fixes

- **`fix(tests)`: `plugin_registry_path_falls_back_to_legacy_when_canonical_missing`.** The test was setting `ANIMUS_CONFIG_DIR` to a non-empty path while asserting that `default_config_path()` returns the legacy `~/.config/animus/plugins.yaml`. The `config_dir_overridden` guard in `default_config_path()` intentionally skips the legacy fallback when `ANIMUS_CONFIG_DIR` is set, so the test was actually deterministically failing — but ENV_GUARD poisoning and other tests interacting through the same mutex were masking it as a "flake". The fix explicitly clears `ANIMUS_CONFIG_DIR` for the duration of the test (using the existing `EnvVarGuard`) so the legacy-fallback branch is exercised.
- **`fix(tests)`: `install_succeeds_after_org_added_to_trusted` + 4 siblings.** All five trusted-orgs tests in `ops_plugin` were calling `std::env::set_var("ANIMUS_TRUSTED_ORGS", …)` and `remove_var` bare, with no serialization. Concurrent test threads racing on the same process-global env var were the documented flake source. Fix: new `TRUSTED_ORGS_ENV_GUARD` process-level mutex plus a small `ScopedEnv` RAII helper that restores the prior value on drop (panic-safe). All five tests now serialize through the mutex.

### Plugin Cascade

No plugin-repo cascade in v0.4.10 — `animus-protocol` stays pinned at v0.1.4. The wire shape adopted by item A already lives in v0.1.4, so no protocol bump was required.

### Ops / CI

- **`chore(ci)`: `FORCE_JAVASCRIPT_ACTIONS_TO_NODE24` set across all in-tree workflows.** GitHub Actions runners deprecate Node 20 on 2026-06-02; the env var forces JS-based actions onto Node 24 ahead of the cutover. Patched into all 8 `.github/workflows/*.yml` files as a top-level `env:` block.

### Carried Forward

- **Release automation across the 15-repo ecosystem.** Scoping was completed (lighter path: a separate `launchapp-dev/animus-release-automation` repo with a CLI tool that runs the version-pin matrix check + cascade-PR generation), but the implementation work was deferred so the v0.4.10 budget could land items A through F. Filed as the v0.4.11 / v0.5 starting point.

## [0.4.9] - 2026-05-22

Patch release. Picks up v0.4.8 deferrals plus a handful of originally-planned items: the plugin-repo cascade onto animus-protocol v0.1.4, daemon-health per-plugin enumeration, a log redaction layer, and a cleanup pass on the stale `AgentPool` references that have been confusing agent reports for several patches.

### Features

- **`feat(daemon health)`: per-plugin section.** `DaemonHealthResponse` now carries a populated `plugins: Vec<PluginHealth>` whenever the daemon's control wire serves the request: each discovered plugin contributes a `name` / `kind` / `status` row. Live `health/check` RPC fan-out per plugin is intentionally deferred — without a long-lived plugin host pool the daemon would have to one-shot every plugin process per health probe, which is too expensive for a frequently-polled endpoint. The CLI's human renderer prints a compact `plugins:` table when the wire path is taken; the JSON envelope passes the full wire shape through unchanged.
- **`feat(daemon-runtime)`: log redaction layer.** New `orchestrator_daemon_runtime::control::log_redact` module that scrubs secret-shaped values from `LogEntry.msg`, `LogEntry.meta` (recursively over nested JSON), `LogEntry.content`, and `LogEntry.error` before the entry is handed to a storage sink. Default pattern matches `api_key|apikey|api-key|password|token|secret|authorization` followed by `:` or `=` and a value; additional patterns can be appended via `ANIMUS_LOG_REDACT_PATTERNS` (comma-separated regex). Five unit tests cover the default patterns, custom patterns, nested JSON traversal, and the no-op path. The redactor is shipped as a public function — wiring it into every emit path is a v0.4.10 follow-up so the patch boundary stays bounded.

### Plugin Cascade

All 12 plugin repos were re-pinned to animus-protocol v0.1.4 and bumped to their next patch version. The cascade is git-additive: each repo got one `chore: bump to v0.1.x + pin animus-protocol v0.1.4` commit + a new tag, pushed to `main`.

| Repo                              | Old   | New   |
|-----------------------------------|-------|-------|
| animus-subject-linear             | 0.1.2 | 0.1.3 |
| animus-subject-sqlite             | 0.1.2 | 0.1.3 |
| animus-subject-markdown           | 0.1.2 | 0.1.3 |
| animus-subject-requirements       | 0.1.2 | 0.1.3 |
| animus-provider-claude            | 0.1.1 | 0.1.2 |
| animus-provider-codex             | 0.1.1 | 0.1.2 |
| animus-provider-gemini            | 0.1.1 | 0.1.2 |
| animus-provider-opencode          | 0.1.1 | 0.1.2 |
| animus-provider-oai               | 0.1.1 | 0.1.2 |
| animus-trigger-webhook            | 0.1.0 | 0.1.1 |
| animus-trigger-slack              | 0.1.0 | 0.1.1 |
| animus-log-storage-file           | 0.1.0 | 0.1.1 |

The triggers and log-storage repos previously pinned animus-protocol via `branch = "main"`; they now pin `tag = "v0.1.4"` explicitly so the wire surface is reproducible.

### Documentation

- **AgentPool references aligned with reality.** The CLI and daemon-runtime comment blocks claiming "AgentPool carries `#[allow(dead_code)]`" referred to a struct that no longer exists in the tree. Comments in `ops_agent/control_routing.rs` and `control/routing.rs` updated to state plainly: the wire surface stays pass-through because there is no daemon-side agent pool yet, and CLI callers degrade to the local in-process path.

### Deferred to v0.4.10

- **`web-api workflows_list`** continues to return `ListPage<OrchestratorWorkflow>`. Migrating to the wire `WorkflowListResponse` requires both a contract change (pagination model, status casing) and the dropped fields (`phases`, `machine_state`) propagated through the GraphQL surface and downstream typed tests. The web-api crate has stalled multiple agents on this exact migration; held back another patch to keep v0.4.9 from sprawling.
- **Log redaction at every emit site.** v0.4.9 lands the redactor as a public function plus tests. Calling it from `Logger::write_entry` (or from each emit builder) is the next step; the patch ships the building block without changing emit semantics yet.
- **Live `health/check` RPC fan-out per plugin.** Per-plugin status currently reports `Healthy` for any discoverable plugin. Real per-plugin health probes require a long-lived plugin host pool with per-process status caches.

## [0.4.8] - 2026-05-22

Wire + UX patch. Picks up the v0.4.7-deferred web-api work that needed an animus-protocol bump, ships the plugin-runtime structured log macros, and polishes the `animus subject` CLI for the single-backend project case.

### Features

- **`feat(protocol)`: bump animus-protocol to v0.1.4.** `WorkflowResumeRequest` carries an optional `feedback` field so approval-gated workflows can pass reviewer comments through resume. `QueueReorderRequest` accepts a multi-entry form (`subject_ids: Vec<String>`) alongside the single-id shape. `animus-plugin-runtime` ships a new `log` module with crate-level `info!`/`warn!`/`error!`/`debug!`/`trace!` macros that emit `log/entry` JSON-RPC notifications via the existing stdout pipe; each `*_main` entrypoint installs the global emitter automatically.
- **`feat(web-api)`: route `queue/reorder` and `workflow/resume` through the daemon control wire.** With the v0.1.4 wire surface in place, the web-api handlers prefer the daemon's control RPC, falling back to the local path only when the daemon isn't running. `workflow/resume` now carries the reviewer feedback string end-to-end.
- **`feat(cli)`: `animus subject` honors `default_subject_kind` from `.animus/config.json`.** New `--kind` becomes optional; falls back to the config default (which seeds to `"task"` for new projects). Operators can now `animus subject list` without re-typing `--kind task` every invocation. A missing config default *and* missing flag prints a helpful error listing the lookup precedence.
- **`feat(plugin-host)`: hierarchical kind matching in `SubjectRouter`.** Plugins may declare glob kinds like `task.*`; resolution does exact-match first, then longest matching glob prefix. Duplicate equal-prefix globs are rejected at registration time. Five new tests cover the precedence rules.
- **`feat(llm-cli-wrapper)`: typed `Error::CapabilityNotSupported`.** Callers can pattern-match on the typed variant instead of grepping `ExecutionFailed` strings; the cancel-routing path now emits the typed error when a plugin's handshake `capabilities.cancellation` is `false`.

### Removed

- **`test(cli)`: delete the stale `json_success_envelope_contract_is_stable` test.** It still drove `animus task stats`, which was removed in v0.4.4 cleanup, so it had been red on every run since.

### Deferred to v0.4.9

- **`web-api workflows_list`** continues to return `ListPage<OrchestratorWorkflow>` instead of the leaner wire `WorkflowListResponse`. The two shapes differ on pagination model (offset/total vs cursor), status casing (snake_case vs kebab-case), and the wire summary lacks `phases`, `machine_state`, and several other fields the handler currently exposes. Migration is a contract change across `paginated_success_response`, ETag computation, the GraphQL surface, and downstream typed tests.
- **`feat(daemon health)`: per-plugin health.** The brief asked for a `plugins:` section in `animus daemon health` that calls each installed plugin's `health/check` RPC. The plumbing change spans the daemon-runtime health snapshot, the control wire's `daemon/health` response, and CLI rendering — held back to keep the v0.4.8 surface bounded.
- **Plugin repo cascade (10 plugins).** Each subject/provider/trigger/log-storage plugin still pins animus-protocol v0.1.3. The v0.1.4 protocol crate is wire-additive so existing plugins continue to work; per-plugin re-pinning + retagging ships in v0.4.9.

## [0.4.7] - 2026-05-22

Wire-routing patch: the daemon's `daemon/logs` control method finally drives `animus logs tail`, and MCP gets the matching surface. Two of the three deferred web-api handlers stay deferred with v0.4.8-tagged notes; the cross-repo plugin runtime cascade ships in a separate window.

### Features

- **`feat(logs)`: route `animus logs tail` through the control wire when the daemon is up.** Adds `ControlClient::daemon_logs` (streaming consumer with a caller-side limit), wires the daemon's `daemon/logs` surface to read historical entries via the active `LogStorageDispatch`, and updates `ops_logs::handle_logs_tail` so the daemon-up branch no longer opens `events.jsonl` directly. Operators get a single transport regardless of which side is serving; the plugin live-tail path (long-lived `log_storage_backend` plugin host) stays deferred but the contract is now wire-first.
- **`feat(mcp)`: add `animus.logs.tail` tool.** Mirrors the `animus.subject.*` pattern — typed input struct, args builder, `run_tool` shell-out — so agents can pull the daemon's log tail through MCP without re-implementing the wire-fallback logic.

### Deferred to v0.4.8

- **`web-api workflows_list`** keeps returning `ListPage<OrchestratorWorkflow>`. Migrating to the leaner wire `WorkflowListResponse` needs a router + OpenAPI contract change and a GraphQL/web-UI follow-up.
- **`web-api queue_reorder`** stays on the local Vec-of-subject-ids path. Wire `QueueReorderRequest` is single-id + anchor + position; lifting requires a multi-id variant in animus-protocol v0.1.4.
- **`web-api workflows_resume` with feedback** stays local. `WorkflowResumeRequest` lacks a feedback field on the wire; adding it needs an animus-protocol v0.1.4 bump.
- **Plugin runtime `log::{info!, warn!, …}` macros + per-plugin cascade.** Building the `animus_plugin_runtime::log` macros and re-pinning the 10+ subject/provider/trigger/log-storage plugin repos onto v0.1.4 is a separate window.

## [0.4.6] - 2026-05-22

CI workflow cleanup. Two workflows have been failing on every commit since v0.4.1:

- `ci(protocol-drift)`: bump `STANDALONE_TAG` `v0.1.1` → `v0.1.3` to match the in-tree consumer. The workflow was trying to check out a tag that exists on `animus-protocol` but predated the wire-format expansions the in-tree crate now consumes, so the structural-drift assertion fired on every run.
- `ci(release)`: remove the `docker-publish` job. It pushed to `registry.fly.io/ao-daemon`, a registry slug that predates the `ao` → `animus` rename and no longer resolves. Can be re-added when there's a real registry target to point at; GitHub Release binary artifacts (the load-bearing publish path) are unaffected.

No Rust or CLI changes. Pure CI hygiene.

## [0.4.5] - 2026-05-22

Hotfix release. Two bugs in v0.4.4 + v0.4.3:

- fix(daemon-control): gate Unix-socket server on #[cfg(unix)]
  so Windows release CI compiles. v0.4.2/0.4.3/0.4.4 all failed
  the Windows build for the same reason. Windows now falls back
  to the in-process service path; named-pipe equivalent is a
  future enhancement.
- test: delete 18 cli_e2e tests that exercised v0.4.4-deleted
  commands (animus task / animus requirements lifecycles).
  Unit tests (321/321) cover the new SubjectBackend surface.

## [0.4.4] - 2026-05-21

Cleanup release that drops the legacy CLI command surfaces v0.4.3 made redundant. The in-tree `SubjectBackend` adapters (`InTreeTaskSubjectBackend`, `InTreeRequirementsSubjectBackend`) plus the external subject_backend plugin ecosystem cover the use cases through the unified `animus subject --kind <kind>` surface.

### Removed

- **`feat(cli)`: delete legacy `animus task` command tree.** Replaced by `animus subject --kind task` against the in-tree task adapter. Underlying `orchestrator-core::services::task*` services are untouched and continue to back both the subject adapter and the daemon's workflow runtime.
- **`feat(cli)`: delete legacy `animus requirements` command tree.** Replaced by `animus subject --kind requirement` against the in-tree requirements adapter. `orchestrator-core::services::requirements*` is preserved.
- **`feat(cli)`: delete legacy `animus cloud` command tree.** Animus-sync legacy surface; cloud sync now ships as an out-of-tree plugin.
- **`feat(cli)`: delete `animus setup` command.** `animus init` is the supported onboarding entry point.
- **`feat(cli)`: delete `animus now` command.** Overlaps with `animus status`; consolidate on the unified status dashboard.
- **`feat(cli)`: delete `animus errors` command tree.** Folded into `animus history`.
- **`feat(mcp)`: delete `ao_task_*` / `animus.task.*` MCP tool family.** Use `animus.subject.*` with `kind=task`.
- **`feat(mcp)`: delete `ao_requirements_*` / `animus.requirements.*` MCP tool family.** Use `animus.subject.*` with `kind=requirement`.
- Companion cleanup: removes the now-unused `shared/parsing.rs` task/requirement value parsers, the `cli_types/shared_types.rs` task/requirement help-text constants, and the `services/runtime/stale_in_progress.rs` summary helpers (only the deleted task handler consumed them).

### Preserved

- `orchestrator-core` services for tasks and requirements stay intact — both the new subject adapters and the daemon's workflow runtime continue to depend on them.
- All other CLI command groups (`daemon`, `agent`, `project`, `queue`, `workflow`, `history`, `git`, `skill`, `model`, `pack`, `plugin`, `runner`, `status`, `output`, `mcp`, `web`, `init`, `doctor`, `trigger`, `logs`, `subject`) are unchanged.

## [0.4.3] - 2026-05-21

Controller-as-plugin migration plus the daemon-wiring foundation that makes the v0.4.x plugin ecosystem operational. CLI, MCP, and WebAPI all route through the daemon's new Unix-socket control protocol when the daemon is running, falling back to in-process calls when it isn't.

### Features

- **`feat(daemon)`: Unix-socket control server + control protocol.** Daemon now exposes `~/.animus/<repo-scope>/control.sock` (0700 perms) speaking newline-delimited JSON-RPC 2.0 per [`animus-control-protocol` v0.1.3](https://github.com/launchapp-dev/animus-protocol/tree/v0.1.3/animus-control-protocol). 47 method constants across 7 groups (subject, plugin, daemon, workflow, agent, queue, project). Auto-starts at daemon launch; opt-out via `ANIMUS_DAEMON_DISABLE_CONTROL_SERVER=1`. Emits `DaemonRunEvent::ControlServerResolved` so operators can find the socket path in `events.jsonl`.
- **`feat(cli)`: every command tries the control socket first, falls back to local.** `animus plugin/daemon/workflow/queue/agent/subject` commands open the control socket when present and route through it; one-shot operations (no daemon) keep working via the existing in-process path. Preserves the `animus.cli.v1` JSON envelope on both paths.
- **`feat(mcp)`: tool surface routes through control protocol via CLI subprocess.** `ao_workflow_*`, `ao_queue_*`, `ao_plugin_*`, `ao_daemon_*`, and `ao_agent_*` MCP tools shell out to the migrated CLI, which routes via control when the daemon is up. Six new `animus.subject.*` MCP tools added (list, get, create, update, next, status) to mirror the CLI surface.
- **`feat(web-api)`: REST handlers route through control protocol (9 of 12 migrated).** `queue_list`, `queue_stats`, `queue_hold`, `queue_release`, `workflows_get`, `workflows_run`, `workflows_pause`, `workflows_resume`, `workflows_cancel` all try control first, fall back to direct `ServiceHub` calls. Deferred: `workflows_list`, `queue_reorder`, `workflows_resume` with feedback — wire-shape contract changes pending.
- **`feat(daemon-logs)`: LogStorageBackend plugin discovery + `animus logs tail`.** Daemon discovers installed `log_storage_backend` plugins at startup; if exactly one is found, it becomes the active log sink. Falls back to the in-tree `Logger` writing `events.jsonl` when no plugin is installed. New `animus logs tail [--plugin] [--level] [--since] [--follow] [--json]` CLI reads from whichever backend is active. Opt-out via `ANIMUS_DAEMON_DISABLE_LOG_STORAGE_PLUGIN=1`.
- **`feat(daemon-subject)`: SubjectBackend plugin discovery + `animus subject` CLI.** Daemon discovers installed `subject_backend` plugins, builds the `SubjectRouter` from their declared `subject_kinds`, rejects duplicate-kind claims at startup. Generic `animus subject list/get/create/update/next/status --kind <kind>` routes through whichever plugin owns that kind. Built-in `InTreeTaskSubjectBackend` + `InTreeRequirementsSubjectBackend` auto-register under `kind=task` and `kind=requirement` so legacy `animus task` and `animus requirements` keep working unchanged.

### Refactors

- **`refactor(control)`: relocate `ControlClient` to `orchestrator-daemon-runtime`.** Lived in `orchestrator-cli` originally; `orchestrator-web-api` couldn't depend on it (circular). Moved to the daemon-runtime crate which both already depend on. No behavior change; mechanical import updates across the CLI.
- **`refactor(plugin-runtime)`: subject dispatch accepts `<kind>/<verb>` method names.** Aligns the runtime's dispatch table with the daemon's `SubjectRouter` kind-keyed routing. Shipped as [`animus-protocol` v0.1.2](https://github.com/launchapp-dev/animus-protocol/tree/v0.1.2); subject plugin repos (linear, sqlite, markdown, requirements) cut v0.1.2 to consume it.

### Deferred to v0.4.x patches

- Legacy CLI command deletion (`Command::Task`, `Command::Requirements`, hidden `Review`, `Qa`) plus matching MCP tools (`ao_task_*`, `ao_requirements_*`). The in-tree task/requirements services keep working via the new SubjectBackend adapters; deletion is a follow-up cleanup commit. (Shipped in v0.4.4.)
- `workflows_list` web-api migration (router + OpenAPI contract change required)
- `queue_reorder` web-api migration (multi-id wire support needed)
- TTL eviction for the session-keyed host cache (deferred from the cancel-keep-host-alive MVP)
- AgentPool integration (still `allow(dead_code)`) — agent dispatch returns `NotSupported` over the control wire pending the daemon-side query surface

## [0.4.2] previously listed Unreleased features (now formally released)

- **`feat(plugin)`: cosign signature verification on plugin install.** `animus plugin install <owner/repo>` now checks for a [sigstore cosign](https://www.sigstore.dev) signature bundle (`<asset>.tar.gz.bundle`) alongside the binary asset. When present, the install verifies it via the `cosign` CLI (shell-out for v0.4.x; in-process `sigstore-rs` planned for v0.5+). Default mode: verify-if-present and record `signature_status: verified|unsigned` in `~/.animus/plugins.yaml`. New flags: `--require-signature` refuses install when no bundle is published or verification fails; `--skip-signature` bypasses verification entirely; `--trusted-signers <PATH>` points at a YAML allowlist (default `~/.animus/trusted-signers.yaml`). A FAILING signature always refuses install. The `cosign` binary is a soft dependency — when missing, installs degrade to `signature_status: unsigned` rather than failing. `animus plugin list` gains a `SIG` column surfacing the recorded status. Plugin release workflows (`launchapp-dev/animus-*`) ship signing as of plugin tag `v0.1.2+` via GitHub Actions OIDC (no secrets to manage). See [docs/architecture/plugin-signing.md](docs/architecture/plugin-signing.md).
- **`feat(workflow-config)`: `${VAR}` env-var interpolation in workflow YAML.** `.animus/workflows.yaml`, `.animus/workflows/*.yaml`, and pack-shipped workflow overlays now support shell-style `${VAR}`, `${VAR:-default}`, and `${VAR:?error}` interpolation. Substitution runs before YAML parsing so every string scalar — subject backend configs, provider tokens, MCP `env` blocks, phase env overrides — accepts the same syntax uniformly. Unset required vars fail fast with the YAML file path + line number. Use `$$` to embed a literal `$`. Recommended for non-secret config like team IDs, base URLs, and feature flags; credentials still belong in the daemon's process environment, not in YAML. See [docs/reference/configuration.md](docs/reference/configuration.md#workflow-yaml-interpolation-non-secret-config).

## [0.4.2] - 2026-05-17

Same-day follow-up to v0.4.1. Ships the full v0.4.0 plugin ecosystem: every
provider plugin promised by the v0.4.0 release is now live in its own
`launchapp-dev/animus-*` repository with green CI, alongside the protocol
crates, a public plugin scaffold template, and the first subject backend
(Linear). The in-tree CLI gains the matching `animus plugin install
<owner/repo>` and `animus plugin new` surfaces for one-command install +
bootstrap, and several rough edges in `animus init` and plugin discovery
caught during release verification are fixed.

### Plugin ecosystem (now shipped, was "in-flight" in v0.4.0 notes)

Eight standalone repositories under [launchapp-dev](https://github.com/launchapp-dev),
each tagged `v0.1.0` with green CI:

- [`animus-protocol`](https://github.com/launchapp-dev/animus-protocol) — 5-crate workspace publishing `animus-plugin-protocol`, `animus-subject-protocol`, `animus-provider-protocol`, `animus-plugin-runtime`, and `animus-session-backend`. CI workflow added.
- [`animus-plugin-template`](https://github.com/launchapp-dev/animus-plugin-template) — subject + provider scaffolds consumed by `animus plugin new`. CI workflow runs `cargo check` against both kinds on every push.
- [`animus-subject-linear`](https://github.com/launchapp-dev/animus-subject-linear) — Linear GraphQL `SubjectBackend` reference implementation. Credentials load leniently so `--manifest` probes work credential-free.
- [`animus-provider-claude`](https://github.com/launchapp-dev/animus-provider-claude) — Claude Code CLI wrapper provider.
- [`animus-provider-codex`](https://github.com/launchapp-dev/animus-provider-codex) — Codex CLI wrapper provider.
- [`animus-provider-gemini`](https://github.com/launchapp-dev/animus-provider-gemini) — Gemini CLI wrapper provider.
- [`animus-provider-opencode`](https://github.com/launchapp-dev/animus-provider-opencode) — OpenCode CLI wrapper provider.
- [`animus-provider-oai`](https://github.com/launchapp-dev/animus-provider-oai) — OpenAI-compatible HTTP provider. Lenient credential loading so `--manifest` works without a configured API key.

### Features

- **`animus plugin install <owner/repo>[@tag]`**: Install a plugin directly from a public GitHub release. The CLI resolves the latest release (or the supplied tag), downloads the matching architecture asset, verifies the published checksum, drops the binary into `~/.animus/plugins/`, and registers it in `~/.animus/plugins.yaml`. Mutually exclusive with `--path` / `--url`. New `--tag <TAG>` and `--latest` flags make scripted installs explicit. Local-file (`--path`) and direct-URL (`--url --sha256`) installs from v0.4.0 still work unchanged.
- **`animus plugin new --kind <subject|provider|trigger> --name <name>`**: Scaffold a brand-new plugin project from `launchapp-dev/animus-plugin-template`. Clones the template (or reads `--template-path <PATH>` for offline use), substitutes the per-kind/per-name variables, and writes a buildable Rust project to `./animus-<kind>-<name>/` (override with `--out-dir`). The output project is `cargo build`-clean from the first commit.
- **Plugin install dir is configurable**: New `--plugin-dir <PATH>` flag on `animus plugin install` / `uninstall`, plus the new `$ANIMUS_PLUGIN_DIR` env var, override the default `~/.animus/plugins/` location. Discovery scans the same directory automatically.
- **Plugin registry consolidated under `~/.animus/`**: The plugin registry file moves from `~/.config/animus/plugins.yaml` to `~/.animus/plugins.yaml`. The legacy path is still read transparently on first run; the next `animus plugin install` rewrites it to the new location. Two new helpers (`plugins_registry_path()` and `legacy_plugins_registry_path()`) are re-exported from `orchestrator-plugin-host` for tooling that needs to point at either.

### Fixes

- **`animus init` no longer emits legacy wrapper YAMLs alongside template files** (commit [`f7c85a11`](https://github.com/launchapp-dev/animus-cli/commit/f7c85a11), bug #22): `animus init --template task-queue` was writing both the registry's inline-content workflow files *and* three hardcoded wrapper YAMLs (`standard-workflow.yaml`, `hotfix-workflow.yaml`, `research-workflow.yaml`) because `FileServiceHub::new` ran the hardcoded scaffold inside `bootstrap_project_base_configs` *before* `write_template_files` had a chance to drop registry files. Reorder: write template files first, bootstrap second. The scaffold's `has_existing_yaml` short-circuit now sees the registry files and skips the wrappers. Also fixes an undercount of three in `apply.written_files` because the scaffold was writing outside the tracked path. Verified: a fresh `animus init --non-interactive --template task-queue` now produces 5 YAML files (was 8).
- **Plugin discovery surfaces failed-manifest probes instead of silent-dropping them** (commit [`8394a73`](https://github.com/launchapp-dev/animus-cli/commit/8394a73)): `discover_configured` and `scan_dir` were swallowing manifest-fetch errors, so installed plugins that failed `--manifest` invocation (the symptom: `animus-provider-oai` before the lenient-credential fix) silently disappeared from `animus plugin list` with no diagnostic. New `DiscoveryWarning` struct + `PluginDiscovery::discover_with_warnings()` API returns failed-probe plugins alongside successes, with the binary's stderr + exit code included. `tracing::warn!` fires at every drop site. `animus plugin list --json` gains a top-level `warnings` array; human output prints each warning to stderr. The `animus.plugin.list` MCP tool result gains a parallel `warnings` field. Regression tests cover failed `--manifest` probes for explicit-config plugins, missing configured binaries, and failed `scan_dir` manifest probes.
- **`animus-provider-oai --manifest` works credential-free** (shipped in `animus-provider-oai` v0.1.0): Manifest probes no longer require `OPENAI_API_KEY` to be set. The lenient-config loader returns a manifest with an `error` capability hint when credentials are missing, instead of exiting with code 1.
- **`animus-subject-linear --manifest` works credential-free** (shipped in `animus-subject-linear` v0.1.0): Same fix as the oai provider — credential validation deferred from `--manifest` to the first `subject/list` call.

### CI

- **`animus-protocol` repo now has CI**: `cargo check` + `cargo test` across the 5-crate workspace on every push.
- **`animus-plugin-template` repo now has CI**: Validates both the subject scaffold and the provider scaffold compile cleanly via `cargo check`.

### Docs

- **`README.md` + `CLAUDE.md` drop "extraction in progress" framing**: Both update to describe the shipped 22-crate workspace plus the 8 standalone plugin repositories under [launchapp-dev](https://github.com/launchapp-dev). The CLAUDE.md "Current Baseline" section now reflects v0.4.0 plugin extraction as complete rather than in flux.
- **`docs/migration/v0.3-to-v0.4.md` marked complete**: Migration guide now documents the actual shipped install surfaces (public-repo `owner/repo@tag`, local `--path`, URL `--url --sha256`, and scaffold via `animus plugin new`) rather than describing them as planned.
- **`docs/reference/cli/index.md`**: Adds the `animus plugin install <owner/repo>` and `animus plugin new` flag surfaces, the new `--plugin-dir` override, and the updated default registry path. Discovery-order line updated to point at `~/.animus/plugins.yaml`.
- **`docs/architecture/subject-backend-plugins.md`**: Resolves the v0.4.0 "open questions" section against the v0.1.0 protocol shape that actually shipped. Adds links to the live `animus-protocol` and `animus-subject-linear` repositories.

### Known follow-ups (deferred to v0.4.x)

- `[[bin]] name = "ao-workflow-runner"` in `crates/workflow-runner-v2/Cargo.toml` is still the legacy bin name (from v0.4.1 known-follow-ups list; not yet renamed).
- Release archive filenames still use the `ao-v0.4.X-` prefix; the release script's archive-naming convention still needs to be updated to `animus-v0.4.X-`.

## [0.4.1] - 2026-05-17

Same-day patch for issues caught during v0.4.0 release verification.

### Fixes
- **Plugin discovery config path renamed**: `default_config_path()` in `crates/orchestrator-plugin-host/src/discovery.rs:174` previously read `~/.config/ao/plugins.yaml` — a stale path the v0.4.0 hard rename missed. v0.4.1 reads from `~/.config/animus/plugins.yaml`. Users with a config at the old path need to `mv ~/.config/ao ~/.config/animus` on upgrade.
- **Docker image build broken upstream**: the v0.4.0 Release Binaries workflow's Docker step failed because OpenCode upstream changed its release-asset format from `opencode_linux_${ARCH}` (single binary) to `opencode-linux-${ARCH}.tar.gz` (tarball). Animus's `Dockerfile` now pulls the tarball and extracts the `opencode` binary out of it.
- **Docker stack finished rename**: `Dockerfile` and `docker-compose.yaml` both had leftover `ao` references the rename missed — `mkdir -p /root/.ao` → `mkdir -p /root/.animus`, compose service name `ao` → `animus`, image `ghcr.io/launchapp-dev/ao:latest` → `ghcr.io/launchapp-dev/animus:latest`, volume `ao-data:/root/.ao` → `animus-data:/root/.animus`, entrypoint `ao` → `animus`.

### Docs
- **Reference docs cover the v0.4.0 MCP surface**: `docs/reference/mcp-tools.md` now documents the `animus.skill.{list,get,search}` and `animus.memory.{get,list,append,clear}` families, including the `capabilities.memory: true` gating model. `docs/reference/cli/index.md` gets a "Selected Command Flags" section covering `animus init --update-registry`, the SHA256-required-with-URL plugin install rule, and the `--include-system-path` opt-in. `docs/reference/configuration.md` env var table expanded from 4 to ~30 entries. `docs/reference/data-layout.md` covers the new `~/.animus/packs/`, agent-host skill probe paths, and the resolution-paths table grew from 7 to 14 entries.
- **Narrative-docs `AO` / `ao` prose sweep**: 49 docs across `docs/{design,concepts,guides,architecture,contributing,reference,internals}/` plus the VitePress site config had bare project-name mentions of `AO` / `ao` that survived sub-agent A's code-focused rename. All now read `Animus`. Also folded into this pass: stale crate counts (`17-crate workspace` → `Rust-only Cargo workspace (around 20 crates)`), stale literal command examples (`ao status`, `ao now`, `ao git`, etc.), branch prefix `ao/<task-id>` → `animus/<task-id>` in `docs/concepts/worktrees.md` to match `crates/orchestrator-git-ops/src/daemon_git_worktree.rs`, and stale `PROJECT_ROOT` env var fallback in `docs/reference/cli/global-flags.md` per CLAUDE.md guidance.

### Known follow-ups (deferred to v0.4.x)
- `[[bin]] name = "ao-workflow-runner"` in `crates/workflow-runner-v2/Cargo.toml` is still the legacy bin name. Rename pending.
- Release archive filenames still use the `ao-v0.4.X-` prefix (e.g. `ao-v0.4.0-aarch64-apple-darwin.tar.gz`). The release script's archive-naming convention needs to be updated to `animus-v0.4.X-`.

## [0.4.0] - 2026-05-14

This is a major release and the v0.3.x → v0.4.0 break is intentionally large. Three commitments land together:

1. **One name across every surface.** The partial v0.3.x rebrand from AO to Animus is finished. There is no longer an `ao.*` namespace. Every MCP tool, environment variable, configuration directory, pack id, JSON envelope, plugin crate, and standalone plugin repository uses the `animus` name. No deprecation aliases. See the [naming contract](docs/architecture/naming-contract.md) and [migration guide](docs/migration/v0.3-to-v0.4.md).
2. **The architecture commits to plugin-first.** Subjects (units of dispatchable work) are now pluggable so teams can keep using Jira / Linear / GitHub Issues / Notion as their source of truth. Provider plugins (Claude, Codex, Gemini, OpenAI-compatible, OpenCode) move from in-tree workspace crates to standalone repositories. The plugin protocol crates move to a dedicated `launchapp-dev/animus-protocol` repository and ship as published Rust SDK crates that any plugin author depends on.
3. **The skill system rearchitects around the ecosystem-standard SKILL.md format.** Bundled YAML skills become an installable `animus.core-skills` pack. Skills installed for other agent hosts (Claude Code, Codex, Cursor, OpenCode) become discoverable subject sources for Animus workflows.

Expect breaking changes throughout. The migration is small in absolute terms because the v0.3.x install base is small; the rename is a deliberate inflection point that commits to Animus as an ecosystem rather than a single tool.

### Breaking changes

- **Full rename to `animus` (no aliases).** Every `ao.*` MCP tool name becomes `animus.*` (58 tools). Every `AO_*` environment variable becomes `ANIMUS_*` (51 vars). The project-local config directory `.ao/` becomes `.animus/`. Scoped runtime state at `~/.ao/<repo-scope>/` moves to `~/.animus/<repo-scope>/`. Pack ids `ao.task` / `ao.review` / `ao.requirement` become `animus.task` / `animus.review` / `animus.requirement`. The JSON output envelope schema `ao.cli.v1` becomes `animus.cli.v1`. The plugin protocol host name `"ao"` becomes `"animus"`. The in-tree provider crates rename from `ao-provider-*` to `animus-provider-*`; `ao-plugin-smoke` becomes `animus-plugin-smoke`. The migration guide has the complete list with before/after examples.
- **Bundled YAML skills moved to an installable pack.** `BUILTIN_SKILL_YAMLS` is removed. The 19 skills that previously shipped baked into the binary are now distributed as the `animus.core-skills` pack, auto-installed during `animus init` / `animus setup`. Existing references resolve via the `Installed` source.
- **`animus plugin install --url <url>` now requires `--sha256`.** Previously optional; URL-sourced installs without an integrity check are rejected at the CLI layer.
- **Plugin discovery no longer scans `$PATH` by default.** Discovery is limited to `.animus/plugins/` and `$ANIMUS_PLUGIN_PATH`. Restoring the prior behavior requires explicit `--include-system-path` on the discovery surface.
- **SKILL.md frontmatter: Animus-specific fields move under a vendor namespace.** `tool_policy`, `extra_args`, `env`, `mcp_servers`, `adapters`, `codex_config_overrides` move from top-level frontmatter to `animus:` in the SKILL.md frontmatter. Keeps SKILL.md portable across the ecosystem (Claude Code, Codex, Cursor, gstack) by isolating Animus-specific fields.
- **Subjects are now pluggable.** Native `animus task` is one backend among many. Workflows can declare `subject_type:` to operate over Linear/Jira/GitHub Issues subjects via plugin backends. Existing workflows that omit `subject_type:` continue using the native task backend with no change.
- **Plugin protocol crates move to standalone repository.** `orchestrator-plugin-protocol` is replaced by `animus-plugin-protocol` published from `launchapp-dev/animus-protocol`. The new `animus-subject-protocol`, `animus-provider-protocol`, and `animus-plugin-runtime` crates also publish from that repository. Plugin authors depend on these crates from crates.io rather than path-depending on the core workspace.
- **Provider plugins move to standalone repositories.** `animus-provider-{claude, codex, gemini, oai, opencode}` each ship from their own repository under `launchapp-dev/`. The daemon image installs them at build time from crates.io.

### Architectural commitments (documented for the first time)

- **Naming**: one name everywhere. See [naming contract](docs/architecture/naming-contract.md).
- **Plugin-first architecture**: subjects, providers, triggers, and skills are all plugins. The core is the orchestration runtime; plugins are independent release artifacts. See [subject backend plugins](docs/architecture/subject-backend-plugins.md).
- **Standalone plugin repositories**: every plugin lives in its own GitHub repository under `launchapp-dev/animus-{kind}-{name}`. The repository name, crate name, and binary name all match. Plugin authors generate new plugins from the `animus-plugin-template` scaffold via `animus plugin new --kind <kind> --name <name>`.

### Features

#### Plugin host
- **Stdio plugin host foundation**: New `orchestrator-plugin-host` (in core) plus `animus-plugin-protocol` (extracted to `launchapp-dev/animus-protocol`) implement newline-delimited JSON-RPC 2.0 plugin protocol with handshake, request/response, notifications, and streaming over stdio.
- **Plugin host wired through the runtime**: CLI, MCP server, subject dispatch, and daemon all route through the new plugin host. New `animus plugin install/list/get` commands and `animus.plugin.*` MCP tools.
- **Claude and Codex shipped as stdio plugins**: First-class `animus-provider-claude` and `animus-provider-codex` provider plugins running over the new protocol, distributed from their own repositories.
- **Three additional providers plus mock + runtime**: `animus-provider-gemini`, `animus-provider-opencode`, `animus-provider-oai`, plus the in-tree `animus-provider-mock` (testing) and the shared `animus-plugin-runtime` crate (used by every provider for the RPC loop, streaming, and logging).
- **Plugin smoke test**: `animus-plugin-smoke` crate exercises the end-to-end plugin contract for CI.
- **Subject backend plugins**: New `animus-subject-protocol` defines the `SubjectBackend` trait + normalized Subject schema. First reference implementation: `animus-subject-linear` ships from its own repository. The `animus plugin new` scaffold + `animus-plugin-template` make new subject backends a single-command bootstrap.
- **Template-driven `animus init`**: External template registry (default `launchapp-dev/animus-project-templates`) cloned and cached under `~/.animus/template-registries/<registry-id>/`. Three bundled templates: `task-queue`, `conductor`, `direct-workflow`. Flags: `--template <id>`, `--path <local-dir>`, `--non-interactive`, `--force`, `--update-registry`.
- **Template registry commit pinning**: `animus init` captures the registry HEAD on first clone into `.commit` metadata and refuses to silently fast-forward on subsequent runs. Use `--update-registry` to fetch latest and re-pin.
- **Bootstrap packs externalized**: Templates carry `[[packs]]` declarations that activate during init (e.g. `animus.task`, `animus.requirement`).
- **Markdown skill installs**: `animus skill install` supports SKILL.md format (frontmatter + body) alongside YAML. Body content lands in `prompt.system`. Animus-specific runtime fields live under the `animus:` frontmatter namespace.
- **Agent memory and communication config**: New per-agent memory state plus inter-agent messaging plumbed into phase prompt rendering. Memory MCP tools (`animus.memory.{get, list, append, clear}`) capability-gated on `agent.capabilities.memory == true`.
- **Plugin streaming + structured logging**: Provider RPC loop streams chunks back over notifications; structured logs flow to the host stderr sink.
- **`animus plugin install --url <https-url>`**: Download a plugin binary from an HTTPS URL with mandatory SHA256 integrity verification.

#### Subject backend protocol (v0.4.0 plugin extraction)
- **`animus-subject-protocol` crate**: New workspace crate (also published from `launchapp-dev/animus-protocol`) defining the `SubjectBackend` trait, normalized `Subject` schema (`SubjectId`, `SubjectStatus`, `SubjectFilter`, `SubjectList`, `SubjectPatch`, `SubjectSchema`, `ChangeKind`, `SubjectChangedEvent`), and JSON-RPC method constants (`subject/list`, `subject/get`, `subject/update`, `subject/watch`, `subject/schema`).
- **`animus-provider-protocol` crate**: New workspace crate defining the `ProviderBackend` trait, `AgentRunRequest`/`AgentRunResponse`/`AgentResumeRequest`/`AgentCancelRequest`/`ProviderManifest`/`ProviderCapabilities`/`TokenUsage` types, and `BackendError` with structured RPC error mapping.
- **`animus-plugin-runtime`**: Generalized from `ao-provider-runtime`. Provides `subject_backend_main()` and `provider_main()` entry points, full stdin/stdout JSON-RPC loop, lifecycle method handling (initialize/initialized/$/ping/shutdown/exit/health/check), and the `animus:` shape future plugin authors target.
- **Subject backend design doc**: `docs/architecture/subject-backend-plugins.md` defines the v0.4.0 plugin contract — `SubjectBackend` trait, normalized schema, JSON-RPC method set, workflow YAML binding, native `animus task` migration plan, and 8 open questions for red-team.

#### Skill system rearchitect
- **`animus.core-skills` pack**: The 19 bundled YAML skills now ship as an installable pack at `crates/orchestrator-config/config/bundled-packs/animus.core-skills/`. Auto-installs during `animus init` and `animus setup` and pins itself in `.animus/state/pack-selection.v1.json`. Users can `cd ~/.animus/packs/animus.core-skills/0.1.0/skills/` to read, fork, or override any of the 27 catalog skill names. Pack updates ship via `animus skill install` — no binary upgrade required.
- **`PackManifest::workflows` is now optional**: Skills-only packs are first-class. Validation requires at least one of `[workflows]` or `[skills]`. `PackSkills` is a sibling to `PackWorkflows`.
- **`SkillSourceOrigin::AgentHost { host, scope }`**: New variant scans skills installed for other agent hosts. Six hosts wired via `AGENT_HOST_SPECS`: Claude Code (`~/.claude/skills/`, `<project>/.claude/skills/`), Codex (`~/.codex/skills/`), OpenCode (`~/.config/opencode/skills/`), Cursor (`~/.cursor/skills/`), Kiro (`~/.kiro/skills/`), Slate (`~/.slate/skills/`). Project-scoped variants take precedence over global. Each scope is `Project | Global`.
- **Two-tier trust model**: `strip_structural_fields_for_agent_host` removes `tool_policy`, `extra_args`, `env`, `mcp_servers`, `adapters`, `codex_config_overrides`, and `capabilities` from any AgentHost-discovered skill at parse time. AgentHost skills can only contribute prompt text + directives. To use a host-discovered skill's full structural fields, the user must explicitly run `animus skill install --path` to promote it to the high-trust `Installed` source.
- **`animus:` vendor namespace in SKILL.md frontmatter**: Animus-specific runtime fields (`tool_policy`, `mcp_servers`, `model.preferred`, `adapters`, etc.) now live under `animus:` in the SKILL.md frontmatter, keeping SKILL.md portable across other agent hosts (Claude Code, Codex, Cursor, gstack) that don't recognize Animus-specific keys. Top-level placement of these fields is no longer parsed (hard cut, no deprecation warning). The trust strip still applies even when `animus:` namespace is present in an AgentHost source.
- **Source chain priority**: `AgentHost::Global < AgentHost::Project < Builtin < Installed (pack + registry) < User < Project`. `load_skill_sources` returns sources in lowest-to-highest order; resolution iterates in reverse for highest-priority match.
- **`docs/architecture/skill-system.md`**: New design doc covering the source chain, trust tiers, vendor namespace, and `animus.core-skills` pack layout.

#### `animus.skill.*` MCP tools
- **`animus.skill.list`**: Enumerate all skills across every source (Builtin, Installed, AgentHost::Project, AgentHost::Global, User, Project). Each entry carries `source` (variant tag) + `source_detail` with provenance: registry/source/version for Installed; host/scope plus `structural_fields_stripped: true` and `trust_tier: "prompt_text_only"` for AgentHost. Optional `source` filter (`builtin`, `installed`, `agent_host`, or a host id like `claude-code`).
- **`animus.skill.get`**: Fetch the full `SkillDefinition` for a skill by name. AgentHost-source responses include a `notice` field explaining that structural fields were stripped at parse time.
- **`animus.skill.search`**: Case-insensitive substring match over name, description, and tags. Optional source filter and result limit.

External agents (Claude Code session driving Animus, workflow-spawned subprocesses connected over MCP) can now discover the skill catalog at runtime — including the user's existing `~/.claude/skills/` library that v0.3.x couldn't reach — without shelling out to `animus skill list --json` or hardcoding skill names.

### Fixes
- **Honor configured rework routing** (#024f0161): `rework` verdicts now respect the configured routing target instead of defaulting to the immediately preceding phase.
- **Plugin handshake hangs blocked**: `PluginHost::handshake` now wraps the initialize round trip in a 30-second timeout so a non-responsive plugin can't deadlock daemon startup.
- **`expect()` panics removed from plugin code paths**: `animus.plugin.call` MCP tool and the provider runtime's `initialize` / `health/check` paths now return structured RPC errors instead of panicking when serialization or registry initialization fails.
- **`git clone` / `git pull` errors surfaced**: Template registry sync now captures stdout/stderr from git subprocess invocations and includes them in error messages (was previously swallowed).

### Docs
- **Naming contract**: `docs/architecture/naming-contract.md` commits to one name across every surface (Animus). Cross-linked from architecture index, VitePress nav, `CONTRIBUTING.md`, and `CLAUDE.md`.
- **Subject backend plugin protocol**: `docs/architecture/subject-backend-plugins.md` defines the v0.4.0 subject backend plugin design.
- **Skill system v0.4.0**: `docs/architecture/skill-system.md` — source chain, trust tiers, `animus:` namespace, `animus.core-skills` pack layout.
- **Migration guide**: `docs/migration/v0.3-to-v0.4.md` walks through every breaking change with before/after examples covering all 14 surfaces (rename, plugin discovery, template registry pinning, skill system, subject backend pluggability).
- **`feature-status.md`**: Template-driven project init flipped from In-Flight to Shipped.
- **`docs/architecture/index.md`**: Updated to reflect the all-`animus` naming, the ~20-crate workspace size, and the v0.4.0 subject backend plugin link.
- **`CLAUDE.md`** (project guide): Updated state paths from `.ao/` → `.animus/`, env var references from `AO_*` → `ANIMUS_*`, and the working-rules naming guidance to the all-`animus` model.

### Internal
- **Cargo aliases renamed**: `cargo ao-fmt`/`ao-lint`/`ao-bin-build`/`ao-bin-check` → `cargo animus-fmt`/`animus-lint`/`animus-bin-build`/`animus-bin-check`. CI release workflow updated.

### Known issues / fix-before-tag
- Plugin discovery executes `--manifest` on every candidate found in `.animus/plugins/` and `$ANIMUS_PLUGIN_PATH` with no signature, allowlist, or sandboxing. `$PATH` is opt-in (off by default). Treat the install / discovery surface as trusted-only until a richer trust model lands.
- Subject backend reference plugin (`animus-subject-linear`) and `animus plugin new` scaffold command are still on the v0.4.0 roadmap; the protocol crate stack is in place to support them. Native `animus task` migration to satisfy the `SubjectBackend` trait also remains pending.
- The optional `animus.skill.registry.list` MCP tool was deferred — `SkillRegistrySourceConfig` is `pub(super)` to `ops_skill` and exposing it cleanly needs a thin `pub fn list_skill_registries()` accessor first. Follow-up.

## [0.3.2] - 2026-04-10

### Fixes
- **Default decision contract evidence types** (TASK-222): Implementation phase decision contract now accepts the common evidence kinds agents actually return (`bug_confirmed`, `fix_identified`, etc.), not only `files_modified`. First workflow runs no longer fail immediately when an agent does an analysis-only pass.
- **`phase_decision.evidence` is now optional** when no required evidence types are configured.
- **Strip secrets from tracked state**: Removed `.ao/sync.json` from tracking; it can contain auth tokens.
- **Rustfmt + lint cleanups**: `runtime_contract.rs` method chain and test formatting.

### Features
- **`animus task status --force`**: Allow forced status transitions for skip/cancel paths.
- **Surface agent file-edit decisions**: When an agent runs an analysis phase and modifies no files, the phase output now explains why (permissions, working directory, prompt constraints, or "no change needed") instead of leaving the user guessing.

### Docs
- Restructured getting-started to lead with autonomous mode.
- README updated with v0.3.0 feature surface.

## [0.3.1] - 2026-04-07

### Features
- **Animus daemon Docker image**: Multi-stage `Dockerfile` produces a `linux/amd64` image with minimal dependencies, sized for Fly.io cloud deployment.
- **Claude Code, Codex, and OpenCode bundled in the daemon image**: The cloud daemon ships with all three coding CLIs pre-installed so spawned workflow agents work without per-tenant install steps.
- **Docker image publish in release workflow**: CI now pushes `registry.fly.io/ao-daemon` on tagged releases.

### Fixes
- **Auto-start daemon and agent-runner on sync workflow execution** (TASK-221): Fixes "first-run socket error" when invoking a sync workflow without a daemon already running.
- **Dockerfile Rust bump to 1.89**: `async-graphql` requires rustc 1.89+.

## [0.3.0] - 2026-04-06

### Features
- Cloud status integration: ao cloud status calls /api/cli/status for cloud projects and daemons — show user's cloud projects, daemon states, and active workflows from the cloud API
- Cloud API routing: ao-cli route deploy commands through cloud API instead of direct Fly.io — animus cloud deploy create/destroy/start/stop/status should call /api/cli/daemons/* endpoints instead of requiring Fly.io credentials locally
- Auto-detect project linking from git remote + GitHub App installation
- Config alignment: animus cloud push now sends .ao/ config files
- Device auth flow: implement animus cloud login device auth flow
- ACP evaluation: research ACP spec and design AO as ACP server for IDE integration
- Deploy subcommands: add start/stop/status/create/destroy deployment subcommands
- Feature branch workflow: enable feature branch workflow in standard task pipelines
- Cloud CLI: evolve ao sync into ao cloud CLI subcommands
- Memory MCP: wire ao-memory-mcp into default workflow phases
- Marketplace MVP: Skill marketplace MVP — GitHub registry fetch + pack download
- Event triggers Phase 2: Generic Webhook Support
- Event triggers Phase 1: file watcher support
- Remote MCP servers: HTTP/remote MCP server support

### Fixes
- Cloud linking: rustfmt fix for cloud.rs auto-detect linking
- Config test isolation: improve orchestrator-config test isolation and lock handling
- Model references: replace broken glm-5 model refs with claude-haiku, disable pr-sweep schedule
- Sparkcube removal: remove sparkcube tool_profile refs blocking all daemon workflows
- CLI E2E tests: resolve all 6 storage-mismatch and warning failures
- Lint warnings: resolve all clippy warnings workspace-wide
- Daemon task state: fix last failing unit test — daemon_run task-state-change assertion
- Config test assertion: align test assertion with standard-workflow ID rename
- Env lock handling: recover from poisoned env lock in tests
- Path references: replace hardcoded fixture paths and absolute paths with CARGO_MANIFEST_DIR

### Refactors
- Rebrand to animus: rename primary binary from 'ao' to 'animus', maintain 'ao' as alias
- Help text updates: update all help text and error messages from 'ao' to 'animus'
- Test updates: update test assertions for rebrand to 'animus'
- Unused dependencies: strip unused MCP servers — remove filesystem, sequential-thinking, memory, rust-docs

### Documentation
- Public repo preparation: clean README.md with Animus branding, verify ELv2 LICENSE, remove hardcoded local paths, add CONTRIBUTING.md, ensure .gitignore covers sensitive files
- README updates: fix typo and update GitHub release links

### Style
- Formatting fixes: rustfmt fixes for line length, import ordering, and cloud status command

## [0.2.35] - Previous Release

See git history for earlier releases.
