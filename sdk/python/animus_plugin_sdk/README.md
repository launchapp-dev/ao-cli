# animus-plugin-sdk (Python)

Python SDK for authoring [Animus](https://github.com/launchapp-dev/animus)
stdio plugins (subject backends today; providers, triggers, transports, and
log storage in later waves).

This is the Python parallel to the TypeScript SDK at
[`sdk/typescript/animus-plugin-sdk`](../../typescript/animus-plugin-sdk).
Both SDKs derive from the same source-of-truth JSON Schemas at
[`schemas/animus-{plugin,subject}-protocol/_all.json`](../../../schemas).

## Install

```sh
pip install animus-plugin-sdk
```

(Not yet published — pin to a git ref while we stabilize 0.1.x.)

## Hello world

```python
# my_plugin.py
from animus_plugin_sdk import (
    PluginKind,
    Subject,
    SubjectCallContext,
    SubjectListParams,
    SubjectListResult,
    define_plugin,
)


class HelloBackend:
    def schema(self):
        return {"kinds": ["task"]}

    def list(self, params: SubjectListParams, ctx: SubjectCallContext) -> SubjectListResult:
        return SubjectListResult(
            subjects=[
                Subject(id="task:1", kind=ctx.kind, title="hello"),
            ],
        )

    def get(self, params, ctx):
        if params.get("id") == "task:1":
            return Subject(id="task:1", kind=ctx.kind, title="hello")
        return None


if __name__ == "__main__":
    define_plugin(
        kind=PluginKind.SUBJECT_BACKEND,
        impl=HelloBackend(),
        name="hello-subjects",
        version="0.1.0",
        description="Hard-coded sample backend",
        subject_kinds=["task"],
        env_required=["MY_API_TOKEN"],
    ).run()
```

Run the plugin directly to drive the JSON-RPC loop on stdin/stdout:

```sh
python my_plugin.py
```

Print the manifest (`--manifest` shortcut, used by `animus plugin install`):

```sh
python my_plugin.py --manifest
```

## Roles

| Role                  | Status (Python SDK 0.1.0)                                   |
| --------------------- | ----------------------------------------------------------- |
| `subject_backend`     | Fully wired — list/get/create/update/status/next dispatched |
| `provider`            | Protocol type only; dispatcher returns `MethodNotFound`     |
| `trigger_backend`     | Protocol type only; dispatcher returns `MethodNotFound`     |
| `transport_backend`   | Protocol type only; dispatcher returns `MethodNotFound`     |
| `log_storage_backend` | Protocol type only; dispatcher returns `MethodNotFound`     |

Same scope as the TypeScript SDK 0.1.0. Pass `kind=PluginKind.PROVIDER` (or
any non-subject kind) today and `define_plugin` will raise `ValueError`.

## Protocol version

This SDK targets `PROTOCOL_VERSION = "1.0.0"`. The handshake validates the
host's advertised version with strict major-version match: a `1.x` plugin
accepts any `1.x` host but rejects `0.x` or `2.x`.

## Wire types

Wire payloads use `pydantic.BaseModel` with `extra="allow"` so unknown
fields round-trip (the Python equivalent of Rust's `Other(String)`
fall-through pattern). The types under `animus_plugin_sdk.types` are
hand-written today and will be replaced by codegen output once we wire
`datamodel-code-generator` against the JSON Schema artifacts. The
`# TODO(codegen)` markers identify the swap-in points.

## Parity with the TypeScript SDK

| Concept             | TS                     | Python                          |
| ------------------- | ---------------------- | ------------------------------- |
| Entrypoint          | `definePlugin(spec)`   | `define_plugin(kind, impl, …)`  |
| Stdio loop          | `createWire()`         | `create_wire()`                 |
| Handshake helpers   | `buildManifest`        | `build_manifest`                |
| Role contracts      | `interface`            | `typing.Protocol`               |
| Wire payload models | TS interfaces          | `pydantic.BaseModel`            |
| Async               | `Promise<T>`           | Sync (Iterable + dispatch)      |

## Development

```sh
cd sdk/python/animus_plugin_sdk
python -m venv .venv
source .venv/bin/activate
pip install -e ".[dev]"
mypy animus_plugin_sdk
pytest -v
ruff check
ruff format --check
```

## License

Elastic-2.0
