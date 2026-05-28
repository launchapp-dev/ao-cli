# Sample Inbound Leads

Five realistic inbound leads for dry-running the `qualify-lead` workflow.
Drop this file into your leads directory (or split it into one lead per
file) and the `animus-subject-markdown` plugin will surface each `##`
block as a separate subject of kind `lead`.

The shape each entry follows:

```
## <lead-id>: <company> - <short context>
status: open
stage: inbound
contact: <name>
title: <job title>
email: <email>
company: <company>
inbound_channel: <demo-form | webinar | cold-list | docs | partner>
received_at: <iso8601>

<body in the lead's voice, or the rep's intake notes>
```

---

## LEAD-2001: Northwind Logistics - VP Eng inbound demo request
status: open
stage: inbound
contact: Priya Mehta
title: VP of Engineering
email: priya.mehta@northwind-logistics.com
company: Northwind Logistics
inbound_channel: demo-form
received_at: 2026-05-26T09:14:00Z

Hi — we're a 350-person logistics platform (Series C, raised in Feb).
Currently running a homegrown internal tools stack and evaluating
vendors to replace it before our next planning cycle. We have a
side-by-side bake-off scheduled for week of June 8 between three
options. I sign on tools up to $80k/yr without going to procurement.
Goal is a pilot in Q3 with a 5-seat rollout, full team (40 engineers)
by end of year if it sticks.

Would like a 45-minute deep-dive on the runtime model and the
self-hosted option specifically — our security review will block any
fully-managed offering.

— Priya

---

## LEAD-2002: Helios Federal - procurement intake (enterprise)
status: open
stage: inbound
contact: Marcus Doyle
title: Strategic Sourcing Manager
email: m.doyle@helios-federal.gov
company: Helios Federal
inbound_channel: cold-list
received_at: 2026-05-27T14:02:00Z

Reaching out from Helios Federal's strategic sourcing office. We
maintain a vendor shortlist for the engineering org (~2,200 devs
across 6 business units) and your name came up in two recent
internal requests.

Before we can move you to the evaluation list we'll need: SOC 2 Type
II report, signed BAA, FedRAMP status (Moderate at minimum), and your
data residency policy for US-East. Please route this to your
enterprise / federal sales contact and copy our compliance lead
(c.tan@helios-federal.gov) on the reply.

No timeline pressure on our side — we refresh the shortlist every
H2. If you don't currently meet FedRAMP Moderate, that's fine,
please indicate roadmap and we'll revisit next cycle.

— Marcus

---

## LEAD-2003: solo developer evaluating open-source
status: open
stage: inbound
contact: Jin Ouyang
title: Founder / Solo Developer
email: jin@ouyang.dev
company: (independent)
inbound_channel: docs
received_at: 2026-05-25T22:41:00Z

Hey — really like what you're building. I'm a one-person shop working
on a side project (no revenue yet) and your open-source plan looks
like a great fit. I'm running everything self-hosted on a tiny VPS.

Two questions:
  1. Is there a hard limit on the free / OSS tier? Couldn't tell from
     the docs whether the daemon caps connections or just leaves it
     to the operator.
  2. If I eventually hit the limit and want to upgrade, what's the
     smallest paid tier? I'd rather not jump straight to "contact
     sales for enterprise pricing" — that usually means I can't
     afford it.

No timeline — just exploring.

— Jin

---

## LEAD-2004: Brightline Studios - webinar attendee follow-up
status: open
stage: inbound
contact: Rachel Vega
title: Director of Design Ops
email: r.vega@brightline.studio
company: Brightline Studios
inbound_channel: webinar
received_at: 2026-05-26T18:30:00Z

Attended the "Autonomous Workflows for Creative Teams" webinar last
week — really enjoyed the live demo. We're a 60-person creative
agency and our design-to-dev handoff is bleeding hours every week.

Honest read: I don't know if this is the right tool for us yet. We
don't have an engineering team — our "developers" are a couple of
contractor frontend folks. But the orchestration story resonated.

Could someone walk us through what a creative-team-focused setup
would look like? If it ends up being engineer-heavy to operate, we
might be a year too early. Budget would need to come out of the ops
line, so under $20k/yr to start.

— Rachel

---

## LEAD-2005: Veridian Health - cold-list account
status: open
stage: inbound
contact: Sam Park
title: Senior Manager, Data Platform
email: s.park@veridian-health.com
company: Veridian Health
inbound_channel: cold-list
received_at: 2026-05-27T11:08:00Z

Saw your tool come up in a peer roundup. Veridian is a regional
health system (~4,000 employees, hospital + clinic network in the
US Midwest). I lead our data platform team (8 engineers).

I have to be honest: I'm not actively looking for new tooling right
now. Our current orchestration stack works, our renewal isn't until
April, and we just finished an internal RFP process for a different
category. I'm willing to take a 20-minute intro call if you can show
me something genuinely differentiated vs Airflow/Dagster — but I'm
not going to be a fast-moving deal.

If "nurture and check in at renewal" is the right move on your side,
that's fine with me.

— Sam
