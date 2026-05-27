# Sample Support Tickets

Five realistic tier-1 tickets for dry-running the `triage-ticket` workflow.
Drop this file into your tickets directory (or split it into one ticket per
file) and the `animus-subject-markdown` plugin will surface each `##` block
as a separate subject of kind `ticket`.

The shape each entry follows:

```
## <ticket-id>: <short subject>
status: open
priority: <best guess from customer wording>
customer: <email or handle>
received_at: <iso8601>

<body in customer voice>
```

---

## TKT-1001: Refund for accidental annual renewal
status: open
priority: high
customer: jamie.lee@example.com
received_at: 2026-05-26T08:14:00Z

Hi — my plan auto-renewed yesterday for the full year ($480) but I'd already
emailed support last month asking to switch to monthly. I never got a reply.
I can't afford to be locked in for 12 months right now and the charge has
already hit my card. Please refund the renewal and switch me to month-to-month.

Reference: invoice #INV-44219.

— Jamie

---

## TKT-1002: Webhook deliveries failing with 502 since this morning
status: open
priority: critical
customer: ops@acme-payments.io
received_at: 2026-05-27T02:47:00Z

We started seeing every webhook delivery come back as 502 Bad Gateway around
02:30 UTC. Our endpoint is healthy — we're getting deliveries from other
providers fine, and our own monitoring shows the endpoint returning 200 to
synthetic checks. This is blocking our reconciliation pipeline so it's
production-impacting for us. Any known incident? Workspace ID is wsp_8K2H7Q.

— Priya, Acme Payments SRE

---

## TKT-1003: Would love a "share read-only link" feature
status: open
priority: low
customer: marcus@studio-north.design
received_at: 2026-05-25T17:02:00Z

Hey team — love the product. One thing that would make my life so much easier:
the ability to share a read-only link to a project with clients who don't have
an account. Right now I have to either screenshot everything or invite them as
a seat which gets pricey for one-off reviews. Even a 7-day expiring link would
be great. Is this on the roadmap?

— Marcus

---

## TKT-1004: Someone logged into my account from Germany — I'm in Toronto
status: open
priority: critical
customer: dana.osei@example.com
received_at: 2026-05-27T11:30:00Z

I just got an email saying there was a successful login to my account from
Frankfurt, Germany. I'm in Toronto and I haven't traveled. I changed my
password immediately but I'm worried — what did they access? Can you tell me
what API keys exist on my account and whether any were created in the last 30
days? Please advise on next steps urgently.

— Dana

---

## TKT-1005: Salesforce sync only pulling 50 records at a time
status: open
priority: med
customer: rev-ops@brightline.co
received_at: 2026-05-26T14:08:00Z

We just enabled the Salesforce integration following your docs. It connects
fine and the initial sync ran, but only 50 contacts came through. We have
~12,000 contacts in Salesforce. Is there a pagination setting I'm missing,
or do we need to use a different API endpoint for large datasets? Happy to
share our connection ID if helpful.

— Sam, RevOps @ Brightline
