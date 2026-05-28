# Web Dashboard Guide

The Animus web dashboard ships as a set of standalone plugins. The CLI no
longer bundles an in-process HTTP server. Instead, `animus web` discovers and
spawns installed `transport_backend` and `web_ui` plugins.

## Installing the Web Stack

Install the default transport + UI plugins in one shot:

```bash
animus plugin install-defaults --include-transports
```

Or install them individually:

```bash
animus plugin install launchapp-dev/animus-transport-http@v0.2.1
animus plugin install launchapp-dev/animus-transport-graphql@v0.2.3
animus plugin install launchapp-dev/animus-web-ui@v0.1.1
```

## Starting the Web UI

Spawn the installed transport plugins and report their bound URLs:

```bash
animus web serve
```

Open the resolved UI URL in a browser:

```bash
animus web open
```

`animus web serve --open` does both at once. If no transport plugins are
installed, the command exits non-zero and prints the install commands above.
If an installed transport or UI plugin declares required env vars in its
manifest, those vars must also be set before `animus web` will spawn it.
Use `animus plugin info --name <plugin-name>` to inspect `env_required`
when startup fails before a URL is reported.

## URL Override

`animus web open --url https://my-tunnel.example` skips plugin discovery and
opens the supplied URL directly. Use `--path /runs` to append a sub-path to
the resolved URL.

## Architecture

The web stack lives in three external repositories under the
[`launchapp-dev`](https://github.com/launchapp-dev) org:

| Repository | Role |
|-------|------|
| `animus-transport-http` | REST + SSE HTTP transport plugin |
| `animus-transport-graphql` | GraphQL transport plugin |
| `animus-web-ui` | React dashboard bundled by a wrapper plugin |

The CLI discovers them through the standard plugin host registry and plugin
search paths, then spawns them via `PluginHost::spawn_with_options` using the
plugin manifest's `env_required` contract. Plugins return their bound URL via
the JSON-RPC `initialize` handshake or the optional `transport/info` call. See
[`crates/orchestrator-cli/src/services/operations/ops_web.rs`](../../crates/orchestrator-cli/src/services/operations/ops_web.rs).
