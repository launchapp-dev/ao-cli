# Animus reference packs

Each pack below is a standalone GitHub repo under
[launchapp-dev](https://github.com/launchapp-dev). They are versioned
independently from `animus-cli` and installable via
`animus pack install launchapp-dev/animus-pack-<name>@<tag>`.

## Reference packs (v0.1.0)

| Pack | Domain | Repo |
|---|---|---|
| customer-support | Tier-1 support ticket triage + draft response | [animus-pack-customer-support](https://github.com/launchapp-dev/animus-pack-customer-support) |
| marketing-outreach | Prospect enrichment + outreach drafting + cadence | [animus-pack-marketing-outreach](https://github.com/launchapp-dev/animus-pack-marketing-outreach) |
| sales-pipeline | BANT lead qualification + discovery drafting | [animus-pack-sales-pipeline](https://github.com/launchapp-dev/animus-pack-sales-pipeline) |
| engineering-backlog | Research → plan → implement → review → test → finalize | [animus-pack-engineering-backlog](https://github.com/launchapp-dev/animus-pack-engineering-backlog) |
| recruiting-pipeline | Candidate screening + debrief synthesis | [animus-pack-recruiting-pipeline](https://github.com/launchapp-dev/animus-pack-recruiting-pipeline) |
| organization-meetings | Per-meeting prep + extract actions + weekly rollup | [animus-pack-organization-meetings](https://github.com/launchapp-dev/animus-pack-organization-meetings) |
| ecommerce-fulfillment | Order processing + return handling | [animus-pack-ecommerce-fulfillment](https://github.com/launchapp-dev/animus-pack-ecommerce-fulfillment) |

Each pack ships:
- `pack.toml` manifest
- `workflows/*.yaml` workflow definitions
- `subjects/sample-*.md` realistic demo subjects (for the markdown subject backend)
- `scripts/setup.sh` idempotent installer
- `docs/architecture.md` + `docs/customizing.md`
- `README.md` with 60-second value prop + 15-minute setup

The pack stubs in this directory (`packs/<name>/README.md`) are kept as
discoverable redirects for anyone browsing the ao-cli tree. They'll be
removed entirely once `animus pack install` + the marketplace surface
are wired up.
