# Sample Meetings

Five realistic meetings for dry-running the `prep-and-followup` workflow.
Drop this file into your meetings directory (or split it into one meeting
per file) and the `animus-subject-markdown` plugin will surface each `##`
block as a separate subject of kind `meeting`.

The shape each entry follows:

```
## <meeting-id>: <short title>
status: ready
type: <1:1 | staff | external | interview | vendor>
attendees: <comma-separated names or handles>
scheduled_at: <iso8601>

prior_context:
<bullets describing what's happened with these attendees recently, what
they last asked for, and any sensitivities to be aware of>

raw_notes_draft:
<rough notes captured during the meeting — used by the extract_actions
and draft_followup phases. Left intentionally unpolished.>
```

---

## MTG-2001: 1:1 with Priya (Eng)
status: ready
type: 1:1
attendees: priya.s (direct report)
scheduled_at: 2026-05-28T15:00:00Z

prior_context:
- Priya took over the billing rewrite in March; shipped phase 1 on time
  but phase 2 is now 2 weeks behind the original plan.
- Last 1:1 she flagged that the on-call rotation is wearing her down —
  she's done 3 of the last 5 weekends.
- She asked for a promo case write-up by end of Q2. We haven't started.
- Strength: ruthless about scope cutting. Growth area: hesitant to push
  back in design reviews.

raw_notes_draft:
priya — phase 2 billing slipping bc the migration scripts keep timing
out on prod-size data. she wants to descope the historical backfill to
last 18 months instead of all-time. agreed — saves ~3 weeks. she'll
write up the decision and send to finance for sign-off this week.

on-call: she's exhausted. I committed to taking her off the next two
rotations and we'll reshuffle. need to talk to mateo about covering.
high priority.

promo: she's nervous about it. agreed I'll draft the first cut of her
case by june 5 and we'll iterate from there. she'll send me her
self-assessment by friday.

she pushed back on the new design review process (says it's slowing
small changes). I think she's right. will raise with the design lead.

---

## MTG-2002: Weekly Staff Meeting
status: ready
type: staff
attendees: leadership-team (eng, product, design, gtm, ops)
scheduled_at: 2026-05-28T17:00:00Z

prior_context:
- Last week we agreed to hold the Q3 roadmap until pricing experiment
  data is in. Data lands Friday.
- Open thread: the marketing site refresh is blocked on a final brand
  decision from the new agency. No movement in 10 days.
- Hiring: we're 2 weeks past the close date on the staff PM req. Search
  firm has sent 3 candidates; none have closed loop with the product
  lead yet.

raw_notes_draft:
pricing experiment — initial data shows annual conversion up 18% with
the new tier structure but monthly churn ticked up too. need 2 more
weeks of data before we call it. eng + product to monitor daily.

brand decision — agreed we're going to give the agency one more week
then bring it in-house. ops to send the deadline note. blocker for
marketing site = brand call.

staff PM — agreed to widen the search and consider senior PMs willing
to step up. recruiter to update brief. product lead to do the loops
this week, no excuses.

q3 roadmap — pushed to next monday once pricing data is in. no scope
cut decisions today.

action: ops to draft the all-hands update on pricing + brand timing.

---

## MTG-2003: QBR with Acme Payments
status: ready
type: external
attendees: priya@acme-payments.io (CEO), miguel@acme-payments.io (CTO), our-account-team
scheduled_at: 2026-05-29T14:00:00Z

prior_context:
- $480k ARR customer, renewal in Q4. Historically NPS 9, but the last
  two months they've raised reliability concerns (the 502 webhook
  incident from May 27 is fresh in their memory).
- Their CTO floated a competitive eval in March. We don't know if it's
  active.
- They asked for a roadmap preview of our new SOC2-Type-2 evidence
  automation.

raw_notes_draft:
acme — they opened with the 502 incident. we walked through the RCA
(upstream provider's TLS cert rotation, fixed within 90 min). they
appreciated the detail. miguel asked for a written postmortem by EOW
— committed.

renewal — priya (their ceo) said the board is asking about
multi-vendor strategy. she said it's not active but she has to be
able to answer the question. we offered to bring our reliability VP
into the next QBR. she liked that.

soc2 — they want the evidence automation in Q3, not Q4. we said we'd
check with the team. real answer is probably Q4-early.

product asks: bulk export of transaction logs (they're doing this
manually monthly), better webhook retry visibility (they want to see
why a delivery was retried). flagged for product review.

decision: we're going to send them weekly reliability digests starting
next week. ops to set up.

---

## MTG-2004: Interview — Staff PM Loop, Sasha Romero
status: ready
type: interview
attendees: sasha.romero (candidate), product-lead, eng-lead, design-lead
scheduled_at: 2026-05-29T18:00:00Z

prior_context:
- Round 2 of 3. Sasha is a senior PM at a similar-stage company. The
  bar for staff is: can they own a full surface area, drive cross-team
  decisions without authority, and write product strategy that the
  exec team can act on.
- Recruiter feedback: strong storyteller, light on quantitative rigor
  in round 1. Probe for metric ownership.
- Reference check pending.

raw_notes_draft:
sasha — strong on product narrative. walked through how she repositioned
her current product from "tool" to "platform" — clear before/after.
positive signal on cross-team. when pushed on metrics: she could name
the north star (activation rate) but couldn't recall the actual numbers
for the last 4 quarters. concern.

scope/ambition: she pushed back on our stated focus areas, said she
thinks we're under-investing in retention. interesting — could be right.

team fit: she asked thoughtful questions about how decisions get made.
no red flags.

next step: round 3 with the cto + a written exercise. product-lead to
draft the exercise prompt by tomorrow. recruiter to schedule.

leaning yes — but want to see the written exercise before committing.

---

## MTG-2005: Vendor Pitch — DataLayer Inc
status: ready
type: vendor
attendees: datalayer-sales-team, our-data-platform-lead, our-finance-lead
scheduled_at: 2026-05-30T16:00:00Z

prior_context:
- DataLayer pitched us in February. We passed because we didn't have
  the data volume to justify the spend. We're now 4x larger.
- They've reduced their starting tier price by 30% since the last
  pitch.
- Our data-platform lead is skeptical — believes we can build the
  ingestion layer in-house. Our finance lead wants us to consider
  buy-vs-build seriously this time.

raw_notes_draft:
datalayer — new pricing is $4k/mo starting tier (was $6k). they
demoed the new schema-evolution tooling — actually pretty slick.
solves the schema-drift problem we hit last month.

data-platform lead: still skeptical. estimates 6 weeks to build
equivalent in-house. finance lead: 6 weeks of senior eng time is
$80k+ fully loaded — datalayer pays back in 18 months even before
opportunity cost.

decision: agreed to a 60-day paid pilot. finance to negotiate the
pilot terms (we want a cancel-for-any-reason clause). data-platform
lead will own the pilot success criteria — 3 specific things they
need to see by day 45 to commit to year 1.

action: finance to send pilot agreement back to datalayer by tuesday.
data-platform lead to write the success criteria doc by friday.
