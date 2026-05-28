# Sample Candidates

Five realistic candidates for dry-running the `screen-candidate` and
`debrief-synthesis` workflows. Drop this file into your candidates directory
(or split it into one candidate per file) and the `animus-subject-markdown`
plugin will surface each `##` block as a separate subject of kind
`candidate`.

The shape each entry follows:

```
## <candidate-id>: <name> - <role>
status: ready
stage: <sourced|screened|interviewing|debriefed|offered|hired|rejected>
source: <referral|inbound|sourced|agency>
role: <role we are screening for>
recruiter: <handle>

<notes — public-context summary, raw interview notes if debriefing, etc>
```

---

## CAND-1001: Priya Raman - Senior Backend Engineer (Referral)
status: ready
stage: sourced
source: referral
role: Senior Backend Engineer (Rust)
recruiter: alex.k

Referred by our staff engineer Dana. Currently at a Series B fintech,
~4 years there, before that ~3 years at a large cloud provider on their
storage team. Public github shows steady activity, several PRs to
tokio-rs and one accepted into rustls. Co-authored an internal-tools
post on her current company's eng blog about migrating a Kafka pipeline
to NATS. Dana says she's "the person I'd hire first if I had budget".
Open to a conversation about Rust-heavy backend roles; not actively
looking.

---

## CAND-1002: Marcus Cho - Junior Full-stack Developer (Inbound)
status: ready
stage: sourced
source: inbound
role: Junior Full-stack Developer
recruiter: alex.k

Applied via the careers page. New grad, CS degree from a state school
(2025). Resume lists one internship (a 12-week stint at a healthtech
startup) and one personal project — a markdown-based knowledge graph
tool with ~80 stars on github. Cover letter mentions specifically liking
how we wrote about our local-first sync model in our recent blog post.
No conference talks, no published writing beyond a few short blog
posts on his personal site about learning Postgres.

---

## CAND-1003: Dr. Helena Vasquez - VP of Engineering (Executive Search)
status: ready
stage: sourced
source: agency
role: VP of Engineering
recruiter: jordan.t

Sourced by an executive search firm we engaged for the VPE role. Current
role: SVP Engineering at a public SaaS company (~$400M ARR, 180-person
eng org). Before that, Director at a unicorn cloud-native company, and
before that an early eng manager at a now-acquired data platform. PhD in
distributed systems. Has given talks at QCon and Strange Loop. Published
a chapter in an O'Reilly book on platform engineering. Search firm flags
that she's specifically interested in earlier-stage roles where she can
"set culture, not inherit it".

---

## CAND-1004: Ben Okafor - Frontend Engineer (Recent Grad, Strong Portfolio)
status: ready
stage: sourced
source: inbound
role: Frontend Engineer (mid-level)
recruiter: alex.k

Self-applied with portfolio link. Recent grad (2025) but resume reads
more senior because of a 2-year gap between high school and college
spent freelancing — three real production sites listed, two of which
are still live and look genuinely impressive (a restaurant-chain
ordering UI and a designer's portfolio site with custom scroll
animations). Active twitter presence, ~3k followers, mostly posts
about CSS and web animation. Wrote a popular thread about CSS
container queries that got picked up by CSS-Tricks. No formal CS
education but ships fast and has taste.

---

## CAND-1005: Diego Aranha - Staff Engineer, Distributed Systems (Passive Source)
status: ready
stage: sourced
source: sourced
role: Staff Engineer, Distributed Systems
recruiter: jordan.t

Sourced via a referral from our hiring manager who knows him by
reputation. Currently a Senior Staff Eng at a large infrastructure
company where he's been for 7 years — early to mid-career employee #~50
who's stayed through scale. Owns their consensus / replication layer.
Gave a talk at RICON 2024 about consensus latency tail-cutting. Several
papers in OSDI and SOSP earlier in his career. Github mostly has small
forks now — most work is internal. Has politely declined two prior
outreaches from our recruiter ("happy where I am right now"). Hiring
manager wants to "give it one more shot" with a personal note.

---

## CAND-2001: Riya Mehta - Senior Backend Engineer (DEBRIEF READY)
status: ready
stage: interviewing
source: referral
role: Senior Backend Engineer (Rust)
recruiter: alex.k

Interview loop complete. Raw notes below for the debrief-synthesis
workflow.

**Interviewer 1 — Sam (Systems Design, 60min):**
Asked about designing a distributed rate limiter. She started by clarifying
SLA + scale (10M req/s, 99.99% availability) before drawing anything,
which I liked. Token bucket with redis cluster as the obvious answer,
but she also drew the failure mode for redis partition and proposed a
local-fallback with eventual reconciliation. Code-review portion: she
spotted the off-by-one in the sliding window implementation immediately.
Quote: "I'd push back on shipping this without a chaos test for the
redis-down case." Strong signal on production thinking. skill_match: 5,
level_fit: 4 (senior, maybe staff in a few years).

**Interviewer 2 — Pat (Behavioral, 45min):**
Asked about a time she had to push back on a PM. Story was clear, with
specific stakes ("we were about to ship a 30% latency regression for a
feature one customer asked for"). Less strong when asked about
mentoring — gave a generic answer about pairing on PRs. Quote: "I
haven't formally mentored anyone, but I try to leave good review
comments." motivation_signals: 4 (clearly cares about craft),
level_fit: 3 (mentoring is a senior-ish gap).

**Interviewer 3 — Jordan (Coding, 60min):**
Implemented a CRDT-based counter from spec. Got the merge function
right on the first try. When the constraint changed (now it has to
support deletion), she paused, thought for 2 minutes, and proposed
adding a vector clock — correct answer. Quote: "I'd rather think than
type." Solid. skill_match: 5, level_fit: 4.

**Hiring Manager Note:** Reference check pending. No risk flags from the loop.
