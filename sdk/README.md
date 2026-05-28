# Animus plugin SDKs

The Animus plugin SDKs are now in their own repos:

- **TypeScript / Node.js** → [launchapp-dev/animus-plugin-sdk-ts](https://github.com/launchapp-dev/animus-plugin-sdk-ts)
- **Python** → [launchapp-dev/animus-plugin-sdk-py](https://github.com/launchapp-dev/animus-plugin-sdk-py)

Both consume the language-neutral JSON Schemas at `schemas/animus-{plugin,subject}-protocol/_all.json` in this repo to keep wire-format parity with the Rust source of truth.

These stubs are kept as discoverable redirects and will be removed entirely in a future release.
