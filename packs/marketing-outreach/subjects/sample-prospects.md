# Sample Marketing Prospects

Five realistic prospects spanning the spectrum from warm inbound to cold
outbound, for dry-running the `triage-prospect` workflow. Drop this file
into your prospects directory (or split it into one prospect per file)
and the `animus-subject-markdown` plugin will surface each `##` block as
a separate subject of kind `prospect`.

The shape each entry follows:

```
## <prospect-id>: <contact name> — <short framing>
status: open
source: <demo_request | webinar_attendee | cold_outbound | account_based | referral>
company: <company name>
contact_name: <full name>
role: <job title>
last_touch_at: <iso8601 or "none">

<free-form context notes — what we know about them, what triggered the
record, anything the SDR or marketing system captured>
```

---

## PRS-1001: Jamie Lee — Series B SaaS founder
status: open
source: demo_request
company: Northwind Analytics
contact_name: Jamie Lee
role: Co-founder & CEO
last_touch_at: 2026-05-26T18:42:00Z

Submitted the demo request form yesterday. Context box said: "Just closed
our Series B ($28M, announced last week) and we're scaling the data team
from 4 to 14 by EOY. We're hitting limits on our current pipeline tooling
and looking at three vendors this quarter — wanted to see your stack before
we shortlist." Company is post-Series-B SaaS in the analytics space, ~60
people. Founder is technical (ex-engineer at a well-known infra company).
No previous touchpoints in the CRM.

— captured by demo_request form

---

## PRS-1002: Priya Subramaniam — Enterprise infra lead
status: open
source: account_based
company: Acme Federal Systems
contact_name: Priya Subramaniam
role: Director of Platform Engineering
last_touch_at: none

Target account on our enterprise ABM list. ~4,000 employees, regulated
industry (defense contractor), platform team owns developer tooling for
~600 engineers. Identified via LinkedIn: she posted a thread last month
about "the build-vs-buy calculus for internal developer platforms"
arguing for buying when the vendor has strong SOC2 + FedRAMP posture.
No prior contact. No mutual connections. Account has been in our ABM
queue for 3 months without progress.

— captured by ABM list import

---

## PRS-1003: Marcus Okafor — Mid-market ops director
status: open
source: cold_outbound
company: Studio North Design
contact_name: Marcus Okafor
role: Director of Operations
last_touch_at: none

Pulled from our ICP list: mid-market design agencies (50-200 employees,
project-based revenue, multi-client tooling needs). No public signal
beyond title fit. Studio North is ~80 people, boutique brand work for
DTC clients. Marcus has been in role 2.5 years per LinkedIn. No funding
news, no hiring posts visible, no engagement with our content. Pure
cold-list outreach — first touch should be light, signal-led, no
calendar push.

— captured by ICP list build

---

## PRS-1004: Dana Osei — Webinar attendee, mid-funnel
status: open
source: webinar_attendee
company: BrightLine Health
contact_name: Dana Osei
role: VP Engineering
last_touch_at: 2026-05-22T15:00:00Z

Attended our "Scaling on-call without burning out your senior engineers"
webinar last Thursday. Stayed for the full 45 min, asked one question in
chat ("how do you handle on-call for hybrid orgs?"). BrightLine Health is
a Series C digital health platform, ~250 engineers. Dana joined 8 months
ago from a well-known fintech. No demo request yet — webinar attendance
is the only signal. Followup should reference the specific question she
asked without re-pitching the webinar content.

— captured by webinar registration + engagement log

---

## PRS-1005: Sam Brightline — Referral from existing customer
status: open
source: referral
company: Lattice Robotics
contact_name: Sam Brightline
role: Head of DevEx
last_touch_at: none

Referred by Alex Chen at Northstar Robotics (a current paying customer,
expansion-tier). Alex sent a Slack DM to their AE saying "you should
talk to Sam at Lattice — they're trying to solve exactly what we solved
with you last year and I told them you'd be the obvious call." Lattice
Robotics is Series A, ~120 people, hardware+software stack. No prior
contact with Sam. First touch should name Alex explicitly in line 1 and
inherit the credibility — no need to re-establish value from scratch.

— captured by AE-logged referral note
