# Ecommerce Fulfillment Pack

Animus runs your tier-1 order ops: validates orders, screens fraud,
drafts customer comms, and routes fulfillment — all reviewable before
send. Stops at shipping label printing because that's an integration
with your WMS.

This is the second non-coding reference pack for Animus (after
`customer-support`). It exists to prove the "self-hosted workflow
engine for AI agents, BYO models, BYO data sources" framing in a
second vertical and to give ecommerce ops teams a working starter
they can fork.

## What you'll need

- `animus` CLI installed (`curl -fsSL https://animus.sh/install | bash`
  or follow [the install guide](../../docs/getting-started/installation.md))
- An Anthropic API key exported as `ANTHROPIC_API_KEY`. The default
  workflow YAML pins every agent to a Claude model (Haiku for the
  cheap validate / route / handoff phases, Sonnet for fraud screening
  and drafting). `setup.sh` only installs `animus-provider-claude`. To
  use OpenAI or Gemini instead, see
  [`docs/customizing.md`](docs/customizing.md) — you'll edit each
  agent's `model` + `tool` and install the matching provider plugin
  before running setup.
- A directory to drop order markdown files into. The pack provides 5
  sample orders you can use to dry-run before wiring up a real source.

## Setup (15 minutes)

From the root of your project, run:

```bash
PROJECT_ROOT="$(pwd)"

# 1. Install plugins, copy the workflows into .animus/workflows/, and
#    split the bundled sample-orders.md into one file per order
#    under orders/inbox/. setup.sh is idempotent; re-running never
#    clobbers edits to existing order files.
bash packs/ecommerce-fulfillment/scripts/setup.sh

# 2. Smoke-test the markdown subject backend
animus subject list --kind order --project-root "$PROJECT_ROOT"

# 3. Start the daemon
animus daemon start --auto-install --project-root "$PROJECT_ROOT"

# 4. Dispatch a process-order run against a single order. See
#    "Dispatching orders — current limitation" below for why we pass
#    --title + --description instead of --subject-id.
ORDER_FILE="$PROJECT_ROOT/orders/inbox/ORD-5001.md"
animus workflow run animus.ecommerce-fulfillment/process-order \
  --title "$(head -n 1 "$ORDER_FILE" | sed 's/^## //')" \
  --description "$(tail -n +2 "$ORDER_FILE")" \
  --sync \
  --project-root "$PROJECT_ROOT"
```

Inspect the run. Animus generates a fresh UUID workflow id per
dispatch and writes outputs under
`~/.animus/<repo-scope>/state/workflows/<workflow-id>/phase-outputs/`,
so you can either:

1. **List recent runs to find the id**, then drill in:

   ```bash
   animus workflow list --project-root "$PROJECT_ROOT"
   animus output phase-outputs \
     --workflow-id <id-from-list> \
     --project-root "$PROJECT_ROOT"
   ```

2. **Capture the id at dispatch time via `--json`** (the CLI envelope
   is `animus.cli.v1` — payload sits under `data`):

   ```bash
   WF_ID=$(animus workflow run animus.ecommerce-fulfillment/process-order \
     --title "..." --description "..." --sync --json \
     --project-root "$PROJECT_ROOT" \
     | jq -r '.data.workflow_id')
   animus output phase-outputs --workflow-id "$WF_ID" \
     --project-root "$PROJECT_ROOT"
   ```

3. **Browse on disk** if you prefer. Persisted per-phase outputs live
   under
   `~/.animus/<repo-scope>/state/workflows/<workflow-id>/phase-outputs/`
   as JSON files, one per phase.

The exact field shapes of `--json` envelope payloads are evolving; if
any of the `jq` paths above return `null` on your install, run the
command without `--json` once to see the human output, then adapt.

## Dispatching orders — current limitation

`animus workflow run` currently accepts `--task-id`,
`--requirement-id`, or `--title` to identify the subject. There is
**no first-class `--subject-id` flag for arbitrary subject kinds
yet**. That means today's dispatch path for this pack creates a
`custom` subject and passes the order title + body inline (the
`--description` flag) — the workflow's LLM phases see the order
content and produce a draft, but the final `flag_for_review` phase
cannot write the status back to the real `animus-subject-markdown`
order record (the run is not associated to the order's
backend-qualified id).

What works today:

- `validate`, `risk_screen`, `route_fulfillment`, and
  `draft_customer_notification` produce real output you can read via
  `animus output phase-outputs`.
- The reviewer takes the draft and acts on it in their WMS / comms
  tool.

What doesn't work end-to-end yet:

- `flag_for_review` updating the order subject's status. The phase
  prompt instructs the agent to call `animus subject status --kind
  order --id <id>` and that will work if the agent passes the correct
  backend-qualified id — but the run envelope doesn't carry it
  automatically because the dispatch went through `--title`.

**The fix is a CLI primitive — `animus workflow run --subject-id <id>
--subject-kind order`** — which would let the markdown backend resolve
the subject context (title + body) up-front and thread the
backend-qualified id through the run envelope. That's the headline
missing primitive this pack surfaces.

## What you get

### `process-order` workflow (5 phases)

| Phase | What it does |
|---|---|
| `validate` | Reads order subject + body. Returns a JSON verdict on whether required fields are present, payment status is acceptable, and (if inventory snapshot provided) stock is available. |
| `risk_screen` | Applies a heuristic fraud rubric (country mismatch, unusual total, velocity, freight-forwarder shipping, etc.) and returns `{ risk_score 0-100, signals[], recommendation: pass | review | hold }`. |
| `route_fulfillment` | Picks `warehouse` / `drop_ship` / `digital` / `split` based on item type, ship destination, and inventory. |
| `draft_customer_notification` | Drafts the customer-facing message (order confirmation, soft acknowledgement for held orders, or shipping notice) with channel-appropriate tone. |
| `flag_order_for_review` | Gate phase that considers BOTH `validate.ok` and `risk_screen.recommendation`. If `validate.ok=false` the order is forced to `blocked` + label `validation-failed` regardless of fraud score. Otherwise: `pass` → `ready` + label `auto-approved`; `review`/`hold` → `blocked` + label `awaiting-fraud-review` (+ `high-risk-hold` for `hold`). Phase id is namespaced (`flag_order_for_review`, not `flag_for_review`) because phase ids share a global keyspace across all loaded YAML files — the customer-support pack already owns `flag_for_review`. |

### `handle-return` workflow (4 phases)

| Phase | What it does |
|---|---|
| `validate_return` | Confirms the order is eligible (return window, item returnability, proof-of-return). |
| `recommend_resolution` | Picks `refund` / `exchange` / `replacement` / `store_credit` and the restocking impact (`restockable` / `refurbish` / `destroy`). |
| `draft_return_message` | Drafts the return-acceptance message — acknowledges friction, states resolution, sets timing expectations. |
| `flag_return_for_review` | Sets status `blocked` + label `return-resolution-pending` plus a per-resolution label so the ops queue can filter. (Phase id differs from process-order's `flag_for_review` because phase ids share a global keyspace across all loaded YAML files.) |

Outputs land under `~/.animus/<repo-scope>/runs/<workflow-id>/` like
any other Animus workflow. Each phase's structured output is captured
separately for audit.

## Customize it

This pack is meant to be forked. The files you'll edit most:

- **`workflows/process-order.yaml`** — change the fraud rubric, the
  route catalogue, the tone guides, or the models per phase (cheap
  Haiku for `validate`/`route_fulfillment`/`flag_for_review`, stronger
  model for `risk_screen` and `draft_customer_notification`).
- **`workflows/handle-return.yaml`** — change the resolution
  catalogue, the return-window policy, or the restocking-impact
  rules.
- **`subjects/sample-orders.md`** — replace with your own orders once
  you've validated the pipeline.

For deeper changes — swapping the LLM, wiring to Shopify/WooCommerce,
integrating a real fraud API, adding a WMS trigger — see
[`docs/customizing.md`](docs/customizing.md).

## What's NOT included

Honesty section. This is a **reference pack**, not a turnkey product:

- **No real ecommerce platform integration.** The pack uses
  `animus-subject-markdown` reading markdown files from a directory.
  To pull from Shopify, WooCommerce, BigCommerce, Magento, or a custom
  storefront, you need a subject backend plugin for that source. See
  `docs/customizing.md` for the shape.
- **No real fraud API.** The `risk_screen` phase applies a heuristic
  rubric via an LLM. For production, swap in Sift, Signifyd, Stripe
  Radar, or Kount — `docs/customizing.md` shows two integration
  shapes.
- **No shipping-label / WMS integration.** The pack stops at "ready to
  ship" — it never prints a label or pushes the order to ShipStation,
  ShipHero, Cin7, or NetSuite. That integration is per-WMS and lives
  outside the autonomous tier.
- **No payments-side action.** `handle-return` recommends a refund but
  doesn't issue one. The ops reviewer triggers the refund in
  Stripe/Adyen/Braintree/etc. after approving the draft.
- **No long-running customer context.** Each order is processed
  independently. There's no cross-order memory ("this customer has
  filed 4 returns this year — flag for senior review") modelled here.
  Wire up a schedule + a dispatcher with per-customer history if
  needed.

If you build any of the above on top of this pack, please open a PR —
the intent is that this pack grows into a library of ecommerce
patterns over time.

## Architecture

See [`docs/architecture.md`](docs/architecture.md) for a 1-page
diagram of how orders flow through both workflows and which plugins
are involved.
