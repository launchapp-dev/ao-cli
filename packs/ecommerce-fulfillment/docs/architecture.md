# Architecture: Ecommerce Fulfillment Pack

A 1-pager on how this pack composes existing Animus primitives. No new
Rust code is required — only workflow YAML + agent prompts + a subject
backend that knows how to read orders.

## Plugins this pack depends on

| Role | Plugin | Why |
|---|---|---|
| Subject backend | [`launchapp-dev/animus-subject-markdown`](https://github.com/launchapp-dev/animus-subject-markdown) | Reads orders as markdown files from a directory. Routes the `order` subject kind. |
| Provider | [`launchapp-dev/animus-provider-claude`](https://github.com/launchapp-dev/animus-provider-claude) (default) or `animus-provider-oai` / `animus-provider-gemini` | Runs the LLM calls for the validator / fraud-screener / router / drafter / handoff agents. |
| Transport (optional) | [`launchapp-dev/animus-transport-http`](https://github.com/launchapp-dev/animus-transport-http) + `animus-web-ui` | Lets the human reviewer browse drafts in a web UI instead of `animus output tail`. |

Install all of the above in one go:

```bash
animus plugin install-defaults --include-subjects --include-transports
animus plugin install launchapp-dev/animus-subject-markdown
# Provider plugins claim reserved tool names (claude, oai, gemini),
# so installation needs --allow-shadow-builtin to acknowledge the
# shadow-builtin override.
animus plugin install launchapp-dev/animus-provider-claude --allow-shadow-builtin
```

(`scripts/setup.sh` runs the install commands for you.)

## How an order flows

The pack ships **two** workflows that share the same subject kind
(`order`) and provider plugin, but model different stages of the order
lifecycle.

### 1. `process-order` — order intake → ready-to-ship

```
+-------------------+          +-----------------------+
| orders/inbox/     |          | animus-subject-       |
|   *.md            |  ---->   | markdown (plugin)     |
+-------------------+          | kind=order            |
                               +-----------+-----------+
                                           |
                                           |  list / get / status
                                           v
+--------------------------------------------------------------+
| Animus daemon                                                |
|                                                              |
|   workflow run animus.ecommerce-fulfillment/process-order    |
|     (per order subject)                                      |
|                                                              |
|   phase: validate            ---> JSON { ok, issues[] }      |
|         agent: Haiku (cheap, structured output)              |
|                                                              |
|   phase: risk_screen         ---> JSON { risk_score,         |
|                                          signals[],          |
|                                          recommendation }    |
|         agent: Sonnet (heuristic reasoning)                  |
|                                                              |
|   phase: route_fulfillment   ---> JSON { route, rationale }  |
|         agent: Haiku                                         |
|                                                              |
|   phase: draft_customer_notification ---> JSON { draft_msg } |
|         agent: Sonnet (tone-aware prose)                     |
|                                                              |
|   phase: flag_order_for_review ---> subject status + labels |
|         agent: Haiku, mutates_state=true                     |
|                                                              |
+----------------------+---------------------------------------+
                       |
                       v
            +----------------------+
            | Human reviewer reads |
            | draft, approves, and |
            | triggers shipping    |
            | label in the WMS     |
            +----------------------+
```

### 2. `handle-return` — order moves to `returned` → resolution-pending

Triggered when an existing order's status moves to `returned`. Same
plugin set, same subject kind — only the workflow phases differ.

```
+-------------------------+
| order subject (status   |
| transitioned to         |
| `returned`)             |
+-----------+-------------+
            |
            v
+-------------------------------------------------------+
| workflow run animus.ecommerce-fulfillment/handle-return|
|                                                       |
|   phase: validate_return       ---> JSON { ok,       |
|                                       issues[] }     |
|         agent: Haiku                                  |
|                                                       |
|   phase: recommend_resolution  ---> JSON {           |
|         resolution: refund | exchange | replacement | |
|                     store_credit,                     |
|         restocking_impact: restockable | refurbish |  |
|                            destroy }                 |
|         agent: Sonnet                                 |
|                                                       |
|   phase: draft_return_message  ---> JSON { draft_msg }|
|         agent: Sonnet                                 |
|                                                       |
|   phase: flag_return_for_review ---> subject status + |
|                                      resolution-specific |
|                                      labels         |
|         agent: Haiku, mutates_state=true             |
|                                                       |
+----------------------+--------------------------------+
                       |
                       v
            +----------------------+
            | Returns reviewer     |
            | approves draft +     |
            | triggers refund /    |
            | restock in payments  |
            | + WMS                |
            +----------------------+
```

## A note on phase-id namespacing

Phase ids share a GLOBAL keyspace across every YAML file the workflow
config compiler loads (see
`crates/orchestrator-config/src/workflow_config/yaml_compiler.rs::merge_yaml_into_config`).
This pack therefore names its handoff phases `flag_order_for_review`
and `flag_return_for_review` — NOT the generic `flag_for_review` —
so it composes cleanly with the `customer-support` reference pack,
which already owns `flag_for_review`. If you fork either pack, keep
your phase ids unique across the project's combined YAML set.

## Where outputs land

Standard Animus paths — nothing pack-specific:

- Run events / artifacts: `~/.animus/<repo-scope>/runs/<run-id>/`
- Per-phase JSON output: `~/.animus/<repo-scope>/state/workflows/<workflow-id>/phase-outputs/<phase-id>.json`
- Workflow snapshots: `~/.animus/<repo-scope>/state/workflows/<workflow-id>/`

Stream in real time with `animus output monitor --run-id <run-id>`. Pull
a structured per-phase snapshot with `animus output phase-outputs
--workflow-id <workflow-id>`.

## Why this pack matters architecturally

This pack proves the "BYO data source" framing for a second
non-engineering vertical (after `customer-support`):

1. **Subject backend is non-engineering.** Markdown files (or, later,
   Shopify / WooCommerce / BigCommerce) are not code, tasks, or
   requirements — they're business records. Animus treats them
   identically to every other subject kind.
2. **Two workflows over one subject kind.** `process-order` and
   `handle-return` both operate on `order` subjects, just at different
   lifecycle stages. The dispatcher picks the workflow based on the
   order's `status` field.
3. **Phases are LLM-only.** No `mode: command`, no shell calls, no
   git ops. The runner is just sequencing LLM calls and capturing
   structured outputs.
4. **The reviewer is the loop closer.** Animus stops at
   `flag_for_review`. The pack never prints a shipping label or
   triggers a refund — those are integrations with WMS / payments
   systems and live outside the autonomous tier.

This pattern (subject backend + LLM-only workflow + human-handoff)
generalizes to expense approvals, content moderation, hiring screens,
and most agency-style ops use cases.
