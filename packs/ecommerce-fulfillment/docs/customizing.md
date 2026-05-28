# Customizing the Ecommerce Fulfillment Pack

This pack is meant to be forked. Here's the map.

## Swap the LLM provider

Each agent block in `workflows/process-order.yaml` and
`workflows/handle-return.yaml` declares `model` and `tool`. Change them
and reinstall the matching provider plugin.

### Use OpenAI everywhere

```yaml
agents:
  order-validator:
    model: gpt-4o-mini
    tool: oai
  fraud-screener:
    model: gpt-4o
    tool: oai
  fulfillment-router:
    model: gpt-4o-mini
    tool: oai
  notification-drafter:
    model: gpt-4o
    tool: oai
  ops-handoff:
    model: gpt-4o-mini
    tool: oai
```

Then:

```bash
# Provider plugins claim reserved tool names (oai → oai-runner,
# gemini → gemini, etc.) so the installer requires
# --allow-shadow-builtin to acknowledge the shadow-builtin override.
animus plugin install launchapp-dev/animus-provider-oai --allow-shadow-builtin
export OPENAI_API_KEY=sk-...
animus daemon start --auto-install   # daemon picks up the env var
```

### Mix Gemini and Claude

Mixed-provider workflows are fine — agents are independent. Set the env
vars for every provider you reference.

```bash
export GEMINI_API_KEY=...
export ANTHROPIC_API_KEY=...
```

## Swap the order source

The pack uses `animus-subject-markdown` as a cheap, file-backed
substitute for a real ecommerce platform. The workflow YAML itself is
source-agnostic — it references the `order` subject kind without caring
where orders come from.

### Shopify / WooCommerce / BigCommerce (planned)

These backends are not yet shipped as plugins. The intended migration
shape:

```bash
# Hypothetical future install
animus plugin install launchapp-dev/animus-subject-shopify
```

The plugin would register `subject_kinds: ["order"]` (or
`shopify.order` if you want to run multiple sources side by side). The
workflow YAML does NOT change — only the plugin behind the `order`
kind changes.

If `order` is already claimed by `animus-subject-markdown`, you have
two options:

1. **Replace** — uninstall the markdown plugin, install the Shopify
   plugin. The kind `order` now resolves to Shopify.
2. **Coexist** — keep markdown registered as `order`, install Shopify
   as `shopify.order`. Run two workflows side by side, or change the
   workflow's subject kind. (Subject kind is resolved at dispatch time
   via `animus subject` CLI flags or `default_subject_kind` in
   `.animus/config.json`.)

### Roll your own

Subject backends are stdio plugins implementing
[`animus-subject-protocol`](../../../crates/animus-subject-protocol).
Useful methods to implement: `order/list`, `order/get`, `order/status`.
See [`docs/architecture/subject-backend-plugins.md`](../../../docs/architecture/subject-backend-plugins.md)
for the protocol contract.

The smallest working backend is ~200 lines of Rust (or any language that
can speak JSON-RPC over stdio).

## Integrate a real fraud-screen API

The pack ships with **heuristic-only** fraud screening — the
`fraud-screener` agent uses an LLM to apply a fixed rubric. For
production you'll want a real fraud service (Sift, Signifyd, Stripe
Radar, Kount, etc.).

Two integration shapes:

### A. Replace `risk_screen` with a command phase

Change the `risk_screen` phase from `mode: agent` to `mode: command`
and call the fraud API directly. The phase output contract stays the
same — `{ risk_score, signals, recommendation }`. Downstream phases
keep working unchanged.

Phase modes accepted by the workflow YAML schema are `agent`,
`command`, and `manual`. The `command` block is an OBJECT (not a
string) with `program` + `args` — see
`crates/orchestrator-config/src/workflow_config/yaml_types.rs::YamlCommandDefinition`.

```yaml
phases:
  risk_screen:
    mode: command
    command:
      program: bash
      args:
        - scripts/sift-screen.sh
        - --order-id
        - {{subject_id}}
      parse_json_output: true
      expected_result_kind: phase_result
    output_contract:
      kind: phase_result
      required_fields:
        - risk_score
        - recommendation
```

### B. Pass API output as input to the heuristic agent

Keep the LLM agent in place but inject the fraud-service verdict into
the prompt so the LLM can combine the API score with the heuristic
signals and produce a final recommendation. Useful when the fraud API
returns a score but you want LLM judgement on the final action.

This requires an upstream phase that calls the API and stores its
output for `risk_screen` to read via the dispatch envelope.

## Add an actual fulfillment-system trigger

This pack stops at `flag_for_review` — it never prints a shipping
label or hits a WMS. Going further requires either:

1. **A command phase the reviewer triggers manually** after approving
   the draft (e.g. `animus workflow run send-to-wms --task-id <id>`),
   wired to your WMS REST API or CSV-drop integration.
2. **A trigger plugin** that watches for reviewer approval (Slack
   reaction, web UI button click) and dispatches the WMS send.

Both are out of scope for v1 of this pack. The reason: WMS
integrations are wildly heterogeneous (ShipStation, ShipHero, Cin7,
NetSuite WMS, custom-on-prem) and each needs its own connector. The
pack's job is to get the order to a clean "ready to ship" state.

### Wire a WMS connector as a follow-up workflow

```yaml
# .animus/workflows/send-to-wms.yaml
phases:
  push_to_wms:
    mode: command
    command:
      program: bash
      args:
        - scripts/wms-push.sh
        - --order-id
        - {{subject_id}}
      timeout_secs: 120
    capabilities:
      mutates_state: true

workflows:
  - id: send-to-wms
    name: Send order to WMS
    phases:
      - push_to_wms
```

Then the reviewer (or a future approval trigger) dispatches it after
the order clears review.

## Change the fraud-risk rubric

Edit the `fraud-screener` system prompt in
`workflows/process-order.yaml`. Keep the
`output_contract.required_fields` list in sync — if you add a
`signal_breakdown` field that downstream phases read, declare it in
the contract.

Example: replacing the heuristic rubric with country-specific risk
weights.

```yaml
fraud-screener:
  system_prompt: |
    You are a fraud-risk screener. Apply these country-tier weights:

      Tier-1 (low risk):  US, CA, GB, DE, FR, AU, JP
      Tier-2 (medium):    BR, MX, IN, AE, SG
      Tier-3 (high):      everything else

    Score = base-heuristic + country-tier-modifier. ...
```

## Change the route catalogue

Edit the `fulfillment-router` system prompt to add/remove routes. If
you add a new route (e.g. `pickup`, `subscription_box`), update both
the system prompt's route list and the `output_contract` description
for `route` so reviewers know what the legal values are.

## Wire up a schedule

To process automatically as orders arrive, add a schedule block to
the workflow file:

```yaml
schedules:
  - id: process-new-orders
    cron: "*/2 * * * *"   # every 2 minutes
    workflow_ref: animus.ecommerce-fulfillment/process-order
    enabled: true
```

You'll also want a dispatcher agent that scans for orders with
`status=new` and enqueues one `process-order` run per order (the
`requirements`/`req-dispatch` pattern in
[`.animus/workflows/requirements.yaml`](../../../.animus/workflows/requirements.yaml)
is the reference implementation).

Likewise for returns, a separate schedule + dispatcher that watches
for orders transitioning to `status=returned` and fires
`handle-return`.
