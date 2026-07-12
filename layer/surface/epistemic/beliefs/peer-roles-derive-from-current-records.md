---
type: belief
id: peer-roles-derive-from-current-records
persona: architect
facets: [mct, peers, authority, routing, ontology]
entrenchment: medium
status: active
endorsed: true
extracted: 2026-07-12
revised: 2026-07-12
---

# peer-roles-derive-from-current-records

Peer roles must be derived from current explicit records; if a role cannot be totally derived, the ontology is missing a record type, and ambient or cached status is never authority.

## Statement

Peer roles must be derived from current explicit records; if a role cannot be totally derived, the ontology is missing a record type, and ambient or cached status is never authority.

## Evidence

- [[session-20260712-112719]] and [[commit-7a13578]] define ImmediateCaller, CapabilityPublisher, EligibleRouteCandidate, and SelectedExecutor as current projections in `layer/allium/mct-peer-ontology.allium`.
- [[session-20260709-091408]] records the audit/remediation pattern that authority-sensitive decisions re-read current records rather than trusting cached admission, publication, or role state.
- [REPORT.md](layer/surface/build/spec-drift-audit/REPORT.md) records the stale-authority failures and the peer-relationship semantics that motivated the ontology.

## Supports

- [[remote-surfaces-are-runtime-evidence]] by treating a published surface as one current evidence term in candidacy rather than a stored candidacy grant.
- [[mother-kernel-decides-adapters-perform]] by requiring role derivation to remain an authority decision over explicit facts.
- [[typed-domain-records-before-algorithms]] by requiring missing authority meaning to become a record type before routing derives a role from it.

## Attacks

<!-- Add beliefs this defeats -->

## Attacked-By

<!-- Add beliefs that challenge this -->

## Applied-In

- [[commit-88625ff]] derives remote candidates from current directional authority and fresh surface evidence instead of persisting a routable-peer status.
- [[commit-5f8f1af]] makes call admission re-evaluate current peer-binding authority rather than treating remembered hello state as a durable caller role.
- [[commit-7a13578]] specifies the complete current-record derivations and prohibits independently issued peer-role statuses.

## Revision Log

- 2026-07-12: Created — metrics computed by `patina scrape`
