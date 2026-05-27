# @launchapp-dev/animus-plugin-sdk

TypeScript SDK for authoring [Animus](https://github.com/launchapp-dev) plugins. Ship a `subject_backend`, `provider`, `trigger_backend`, `transport_backend`, or `log_storage_backend` plugin in TypeScript without reading the Rust source.

> Status: **0.1.0 skeleton.** Subject-backend role is fully wired. Other roles throw at `definePlugin` — see [Roles](#roles).

> Workspace note: this package currently lives in-tree under
> `sdk/typescript/animus-plugin-sdk/` as a staging area for the v0.5 plugin SDK
> wave. It will be extracted to `launchapp-dev/animus-plugin-sdk-ts` before
> publish. The Rust workspace is unaffected — `cargo` and the in-tree plugin
> host still build and test independently of this directory.

## Install

```bash
npm install @launchapp-dev/animus-plugin-sdk
```

Requires Node 20+.

## Hello world: a subject backend

```ts
#!/usr/bin/env node
import { definePlugin } from '@launchapp-dev/animus-plugin-sdk';

definePlugin({
  kind: 'subject_backend',
  name: 'animus-subject-hello',
  version: '0.1.0',
  description: 'One hard-coded subject — proof the SDK works',
  subject_kinds: ['task'],
  impl: {
    // Return `subjects` (not `items`). The SDK auto-fills `fetched_at` and
    // backstops missing `status`/`created_at`/`updated_at` for demos —
    // production backends should set them from the source-of-truth.
    list: () => ({
      subjects: [
        {
          id: 'task:HELLO-1',
          kind: 'task',
          title: 'Hello from TS!',
          status: 'ready',
          created_at: new Date().toISOString(),
          updated_at: new Date().toISOString(),
        },
      ],
    }),
    // `kind` is parsed from the inbound method (e.g. `task/get`) and supplied via ctx.
    get: ({ id }, ctx) => ({
      id,
      kind: ctx.kind,
      title: 'Hello from TS!',
      status: 'ready',
      created_at: new Date().toISOString(),
      updated_at: new Date().toISOString(),
    }),
  },
}).run();
```

The Animus daemon dispatches subject calls as `<kind>/<verb>` (e.g. `task/list`, `task/get`) — the SDK routes those to your `impl` automatically and exposes the kind via `ctx.kind`.

Save as `bin/index.mjs`, mark executable, point `pack.toml` at it, then:

```bash
animus plugin install ./pack.toml
animus subject list --kind task
```

## What the SDK does for you

- Handles `--manifest` / `--help` CLI shortcuts (the Animus host calls `--manifest` during install).
- Runs the newline-delimited JSON-RPC 2.0 loop on stdin/stdout.
- Implements `initialize`, `$/ping`, `health/check`, `shutdown`, `exit`.
- Validates the host's `protocol_version` against the plugin's protocol version (strict major-version match).
- Dispatches subject-backend methods (`<kind>/list`, `<kind>/get`, optional `<kind>/create`, `<kind>/update`, `<kind>/status`, `<kind>/next`) to your `impl` and supplies the resolved `kind` via `ctx`.
- Auto-fills `fetched_at` on subject list results so authors can return just `{ subjects: [...] }`.
- Advertises `subject_kind:<kind>` capability tokens in the manifest so the daemon's preflight + doctor recognize coverage without spawning the plugin.
- Honors the host's `exit` notification (sent after `shutdown`) so plugins exit cleanly instead of being force-killed at grace-period.
- Surfaces unknown methods as proper JSON-RPC `MethodNotFound` errors.

Stdout is reserved for protocol frames. The SDK writes diagnostics to stderr; you should too.

## Roles

| Kind                    | Supported in 0.1.0 | Notes                                                            |
| ----------------------- | ------------------ | ---------------------------------------------------------------- |
| `subject_backend`       | Yes                | MVP target — fully usable.                                       |
| `provider`              | No (throws)        | `definePlugin({kind: 'provider', ...})` throws — `agent/run` dispatch isn't wired yet and the host would route real agent calls to a plugin that can't answer. Use the Rust SDK in the meantime. Coming in a later wave. |
| `trigger_backend`       | No (throws)        | Same — `definePlugin` throws until `trigger/watch` is wired.     |
| `transport_backend`     | No (throws)        | Same.                                                            |
| `log_storage_backend`   | No (throws)        | Same.                                                            |

The role-shape TypeScript interfaces (`Provider`, `TriggerBackend`, `TransportBackend`, `LogStorageBackend`) are still exported so authors can pre-write impls against the contract — `definePlugin` will accept them once the host dispatch is wired in a later wave.

## Scripts

```bash
npm install
npm run build       # tsc → dist/
npm run typecheck   # strict-mode tsc
npm test            # vitest
```

## License

Elastic License 2.0.
