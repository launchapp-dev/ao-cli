# Sample Ecommerce Orders

Five realistic orders for dry-running the `process-order` (and
`handle-return`) workflows. Drop this file into your orders directory
(or split it into one order per file) and the `animus-subject-markdown`
plugin will surface each `##` block as a separate subject of kind
`order`.

The shape each entry follows:

```
## <order-id>: <short subject>
status: <new | verified | fulfilling | shipped | delivered | returned>
channel: <web | marketplace | wholesale>
customer_email: <email>
ship_to_country: <ISO 3166-1 alpha-2>
billing_country: <ISO 3166-1 alpha-2>
total: <currency string>
payment_status: <paid | authorized | pending | failed>
received_at: <iso8601>

line_items:
  - sku: <sku>
    title: <short title>
    quantity: <n>
    unit_price: <currency string>
    physical: <true|false>

<body / notes in operator voice>
```

---

## ORD-5001: Routine domestic web order
status: new
channel: web
customer_email: nina.park@example.com
ship_to_country: US
billing_country: US
total: $84.50
payment_status: paid
received_at: 2026-05-27T14:22:00Z

line_items:
  - sku: SKU-1001
    title: Cedar candle, 8oz
    quantity: 2
    unit_price: $32.00
    physical: true
  - sku: SKU-2210
    title: Matchbook set
    quantity: 1
    unit_price: $12.50
    physical: true

Nothing unusual — repeat customer (3 prior orders), domestic ship,
billing and shipping addresses match, in-stock. Should auto-approve
and route to the primary warehouse for same-day pick.

---

## ORD-5002: High-value international order — likely review
status: new
channel: web
customer_email: arjun.mehta+orders@example.com
ship_to_country: AE
billing_country: US
total: $2,840.00
payment_status: authorized
received_at: 2026-05-27T16:05:00Z

line_items:
  - sku: SKU-9300
    title: Limited-edition leather weekender
    quantity: 1
    unit_price: $1,890.00
    physical: true
  - sku: SKU-9301
    title: Matching dopp kit
    quantity: 1
    unit_price: $620.00
    physical: true
  - sku: SKU-1101
    title: Premium care kit
    quantity: 1
    unit_price: $330.00
    physical: true

First-time customer. Billing card issued in US, ship-to is Dubai.
Order total ~6x web channel norm. Payment is authorized, not yet
captured. This should land in the fraud-review queue (recommendation
likely `review` or `hold`).

---

## ORD-5003: Digital download — software license
status: new
channel: web
customer_email: dev@studio-frost.io
ship_to_country: GB
billing_country: GB
total: $149.00
payment_status: paid
received_at: 2026-05-27T09:51:00Z

line_items:
  - sku: SKU-DL-220
    title: Pro plan annual license (digital)
    quantity: 1
    unit_price: $149.00
    physical: false

No shipping required — digital delivery only. Should route to the
`digital` fulfillment path and the notification should include the
license-key delivery instructions (without inventing what the actual
key is — that's a follow-up integration).

---

## ORD-5004: Custom-configured product — delayed ship
status: new
channel: wholesale
customer_email: purchasing@redwood-furnishings.com
ship_to_country: CA
billing_country: CA
total: $11,420.00
payment_status: authorized
received_at: 2026-05-26T19:38:00Z

line_items:
  - sku: SKU-CFG-7700
    title: Custom oak dining table, 96in, walnut stain
    quantity: 4
    unit_price: $2,650.00
    physical: true
  - sku: SKU-CFG-7710
    title: Custom matching bench, walnut stain
    quantity: 4
    unit_price: $205.00
    physical: true

Wholesale PO #PO-2026-0418 against an existing account. Items are
made-to-order — 4-6 week lead time. Should route to `drop_ship`
(supplier-built) and the notification should set the wholesale-tone
ship-date expectation honestly (no "ships in 1-2 days" boilerplate).

---

## ORD-5005: Obvious-fraud pattern — should hold
status: new
channel: web
customer_email: free.shopper.7741@protonmail.com
ship_to_country: RU
billing_country: US
total: $4,610.00
payment_status: authorized
received_at: 2026-05-27T03:17:00Z

line_items:
  - sku: SKU-9300
    title: Limited-edition leather weekender
    quantity: 2
    unit_price: $1,890.00
    physical: true
  - sku: SKU-9301
    title: Matching dopp kit
    quantity: 1
    unit_price: $620.00
    physical: true
  - sku: SKU-1101
    title: Premium care kit
    quantity: 1
    unit_price: $210.00
    physical: true

Multiple signals: first-time customer, throwaway-looking email
address, billing US + ship-to RU (mismatched country), multiple
high-value items in a single order, ordered at 03:17 UTC. Should
score 80+ and recommend `hold` with status=blocked + label
`awaiting-fraud-review` + `high-risk-hold`.
