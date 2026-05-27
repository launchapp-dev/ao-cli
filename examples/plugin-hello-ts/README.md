# Hello-world Animus plugin (TypeScript)

A complete, runnable example of an [Animus](https://github.com/launchapp-dev/animus)
plugin written in TypeScript against `@launchapp-dev/animus-plugin-sdk`.

It is a `subject_backend` plugin: when the daemon asks "give me a list of
subjects of kind `hello_world_demo`", this plugin returns three hard-coded
items. That is enough to prove the SDK works end-to-end — the host spawns
your Node process, exchanges the JSON-RPC `initialize` handshake, and routes
`animus subject list --kind hello_world_demo` to your code.

Use this as the template for your own TS plugin. Fork it, swap the three
hard-coded items for real backend calls (Linear, Notion, your custom API),
and ship.

> **Status (wave T):** This example ships in parallel with the SDK itself
> (`sdk/typescript/animus-plugin-sdk/`). If you cloned the repo before the
> SDK lands, `pnpm install` will fail with `LINKED_PKG_DIR_NOT_FOUND` —
> that is expected. Once the SDK wave merges into `main`, the file-path
> dependency in `package.json` resolves and the steps below work end to
> end. Type imports marked with `TODO: confirm after SDK merge` in
> `src/index.ts` should be reviewed once the SDK exports stabilize.

---

## Prerequisites

- **Node.js >= 20** (the SDK targets ES2022 + native `fetch`).
- **pnpm** (or npm / yarn — examples use pnpm).
- **Animus >= 0.4.13** installed and on `$PATH` (`animus --version` to check).
- The Animus daemon should be running for the verification step
  (`animus daemon start --auto-install` if not).

## Build

```bash
cd examples/plugin-hello-ts
pnpm install
pnpm run build
ls dist/   # expect index.js (the single-file ESM bundle)
```

The build uses `tsup` to bundle `src/index.ts` plus the SDK into one ESM
file, so the install layout is just two files: the launcher script and the
bundle. No `node_modules` need to be deployed.

## Install (dev workflow)

Animus's production `animus plugin install` flow expects a cosign-signed
binary downloaded from a GitHub release. For local TS-plugin iteration
that is overkill — drop the launcher + bundle into the plugin dir manually:

```bash
pnpm run install:local
```

Under the hood that copies:

```
~/.animus/plugins/animus-plugin-hello-ts        # bash launcher (exec node)
~/.animus/plugins/animus-plugin-hello-ts.d/
  └── index.js                                  # bundled JS
```

The executable name **must** start with `animus-plugin-` or
`animus-provider-` — that is the prefix Animus's `scan_dir` filter looks
for. (See `crates/orchestrator-plugin-host/src/discovery.rs`.)

> **Note on signing.** Cosign keyless signing for non-Rust plugins is an
> open question for a future SDK wave. For now the dev install path skips
> verification entirely; production-grade TS plugins will eventually
> publish a signed tarball through the same release pipeline as Rust
> plugins.

Animus's discovery scanner does **not** look in `~/.animus/plugins/` by
default — it only does so when the `ANIMUS_PLUGIN_DIR` env var is set, or
when the plugin is registered in `~/.animus/plugins.yaml` by the official
installer. For this dev install path, export the env var so both your
shell and the daemon see it, then start the daemon:

```bash
export ANIMUS_PLUGIN_DIR="${HOME}/.animus/plugins"
animus daemon stop && animus daemon start
```

(There is no `animus daemon restart` subcommand — `stop && start` is the
supported pattern.)

## Verify

```bash
# 1. The host should see the plugin in its discovery list.
animus plugin list
#    expect a row for `animus-plugin-hello-ts` with kind=subject_backend

# 2. Inspect the manifest the host probed at install time.
animus plugin info --name animus-plugin-hello-ts
#    expect protocol_version, capabilities, subject_kinds = ["hello_world_demo"]

# 3. Query the subject_backend through the unified subject CLI.
animus subject list --kind hello_world_demo
#    expect three rows: hello_world_demo:1 ("Read the README"),
#                       hello_world_demo:2 ("Modify a subject"),
#                       hello_world_demo:3 ("Ship your own plugin")
```

You can also probe the manifest directly without going through the daemon:

```bash
pnpm run manifest
#    prints the same PluginManifest JSON that --manifest would emit
```

## Iterate

1. Edit `src/index.ts` (e.g. add a fourth subject, or change the title).
2. `pnpm run build`
3. `pnpm run install:local`
4. `animus daemon stop && animus daemon start`
5. `animus subject list --kind hello_world_demo`

The whole loop is under five seconds.

## Troubleshooting

**`animus plugin list` doesn't show the plugin.**

- Confirm the launcher is executable: `ls -l ~/.animus/plugins/animus-plugin-hello-ts`
  should show `-rwx`. If not: `chmod +x` it.
- Confirm the launcher's filename starts with `animus-plugin-`. The
  discovery scanner ignores anything else.
- Run `pnpm run manifest` to confirm the bundle prints valid JSON on
  stdout and exits 0. If it errors, the daemon's manifest probe will too.

**Host says "plugin not responding" / "manifest probe failed".**

- Check daemon stderr: `tail -f ~/.animus/<repo-scope>/daemon.log`
  (or the path printed by `animus daemon status`). Manifest-probe failures
  log the plugin's stderr.
- The probe runs with `env_clear()` and a 5-second timeout. If your
  plugin needs an env var to print its manifest, that is a bug — the
  manifest must be static.
- Try running the launcher directly to repro:
  `~/.animus/plugins/animus-plugin-hello-ts --manifest`
  should print one JSON line and exit immediately.

**`animus subject list --kind hello_world_demo` returns nothing.**

- Run `animus plugin info --name animus-plugin-hello-ts` and confirm
  `subject_kinds` includes `hello_world_demo`. If it doesn't, the daemon
  won't route the call here.
- Tail daemon logs while running the list call — JSON-RPC parse errors
  or method-not-found responses will show there.

**Bundle not found at `<path>`.**

- You ran `install:local` without `pnpm run build` first. Build, then
  re-install.

---

## What this example does NOT cover

- **`subject/update`, `subject/watch`, mutating backends** — the example
  is read-only on purpose. Extend `backend` in `src/index.ts` with extra
  methods when the SDK exposes them.
- **Provider plugins** (LLM CLIs) — same SDK, different `kind`. A
  hello-world provider example is a separate ticket.
- **Cosign signing for TS plugins** — open question. See note above.
- **Publishing to a registry** — `pnpm publish` works for the package, but
  the binary distribution channel for non-Rust plugins is TBD.

## File layout

```
examples/plugin-hello-ts/
├── package.json                       Node project + scripts
├── tsconfig.json                      Strict-mode TS config
├── plugin.toml                        Static plugin metadata (mirror of --manifest)
├── README.md                          (this file)
├── src/
│   └── index.ts                       ~30 lines — definePlugin call + 3 subjects
└── scripts/
    ├── animus-plugin-hello-ts         Bash launcher (exec node <bundle>)
    └── install-local.sh               Copies launcher + bundle into ~/.animus/plugins
```
