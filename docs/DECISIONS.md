# Decision log

Every design decision we could trace, with its date, its reason, what was
rejected, and where it is written down. Newer entries do not silently override
older ones; when one supersedes another, both say so.

Status values:

- **decided**: settled by the owner. Change it only by a new entry.
- **built**: decided and on `main`.
- **decided, not built**: settled, but `main` does not do it yet. Do not read
  README or ROADMAP wording about these as a description of current code.
- **proposed**: written down and argued, not settled.
- **future**: named so nobody mistakes it for current scope. Not planned now.

Dates are America/New_York. PR merge dates are converted from GitHub's UTC
timestamps. Entries whose only source was the owner's chat with an agent say
so; those are the ones most worth confirming.

Abbreviations: "(a)–(f)" are the six locked decisions in
[design/admin-keys.md](design/admin-keys.md#decided). Historical entries that
say "Product map" refer to the now-frozen
[`layer/allium/mct-product-map.allium`](../layer/allium/mct-product-map.allium).

---

## 1. Authority model

### D-01 Mother holds all authority; running code gains none

- **Date:** foundational (before 2026-07).
- **Status:** built.
- **Decision:** Mother admits every call and grants every capability. A Child
  computes and holds only what Mother handed it. A Toy performs an effect only
  under a current `ToyGrant`. The kernel decides; adapters gather facts and
  perform effects the kernel already authorized.
- **Why:** the product rule is that code should not gain power merely because
  it is running.
- **Rejected:** sandboxing the environment and trusting whatever runs inside it
  (the model used by environment sandboxes; see [PRIOR-ART.md](PRIOR-ART.md)).
- **Source:** [README](../README.md), [`layer/core/what-is-mct.md`](../layer/core/what-is-mct.md), product map.

### D-02 Fail closed everywhere

- **Date:** foundational.
- **Status:** built.
- **Decision:** unknown, expired, revoked, stale, mismatched or malformed state
  is a typed denial. Unconfigured WIT imports refuse to instantiate. A manifest
  `needs` entry is a request and grants nothing. Denials carry a precise
  internal reason and a vague external message ("not authorized").
- **Why:** a permissive default is a hole you only find after it is used.
- **Source:** README security model, product map.

### D-03 WASM components are the only local Child

- **Date:** landed by 2026-10-14 (merged through #36).
- **Status:** built.
- **Decision:** a local Child is a WASM component under wasmtime with deadlines
  and memory caps. Native and JVM software integrates through authenticated
  ingress, a remote Mother, or an explicitly trusted Mother-side adapter. Old
  process/JVM records stay readable and inert.
- **Why:** one confinement boundary. Process-backed Children had no comparable
  confinement.
- **Rejected:** process and JVM local Children (removed by subtraction).
- **Source:** [`layer/surface/build/feat/wasm-only-local-child/`](../layer/surface/build/feat/wasm-only-local-child/), ROADMAP standing backlog.

### D-04 The ledger is authority; logs, metrics and the database are projections

- **Date:** foundational; reaffirmed 2026-10-18 in the child-start note.
- **Status:** built for Toy, Watch and admin-key authority. **Not yet true for
  every fact**: Child approvals and assignments and peer bindings still live in
  daemon config, and restart recovery still reads in-flight work from SQLite.
  See [OPEN-QUESTIONS.md](OPEN-QUESTIONS.md).
- **Decision:** the hash-chained observation ledger is the source of truth.
  Anything else is rebuilt from it or checked against it.
- **Why:** one append-only, verifiable record that an auditor can replay.
- **Source:** README, product map (`MctLocalFirstObservationLedger`), [design/admin-keys.md](design/admin-keys.md#ledger-append-hash-replay).

### D-05 Append before effect; an uncertain append poisons the writer

- **Date:** foundational; batching changed 2026-10-14 (#36).
- **Status:** built.
- **Decision:** facts required before an effect are acknowledged (fsynced)
  before the effect starts. A failed write or sync poisons the single writer and
  the effect is denied. Each writer command is one batch with one `sync_data`.
- **Why:** an effect with no durable record cannot be audited or recovered.
- **Source:** product map (`BeforeEffectRequiresAcknowledgedCommit`,
  `UncertainAppendPoisonsWriter`, `OneWriterDefinesLocalOrder`), #36.

### D-06 Extend with a Toy, never fork the standard

- **Date:** 2026-10-14.
- **Status:** decided (a principle; individual Toys below are proposed).
- **Decision:** standard WASI interfaces (`wasi:random`, `wasi:clocks`,
  `wasi:filesystem`, `wasi:http`) keep their standard meaning. Anything extra,
  such as deterministic randomness or recorded time, is its own Toy beside the
  standard.
- **Why:** patina-wasi-pi served seeded bytes from the secure random source,
  which breaks the WASI guarantee and made secure output predictable.
- **Rejected:** host overrides that replace a standard import with
  deterministic behaviour.
- **Source:** [design/admin-keys.md](design/admin-keys.md#proposed-replay-toy), [design/pi-durable-child.md](design/pi-durable-child.md#rules-that-stay-in-force).

## 2. The six locked decisions (a)–(f)

All six were locked on 2026-10-14 in the admin-keys design. Full text:
[design/admin-keys.md](design/admin-keys.md#decided).

### D-07 (a) Admin keys

- **Date:** 2026-10-14.
- **Status:** decided; partly built (ledger facts #48 and #64, route-scope check #51).
- **Decision:** the admin side makes the keypair; Mother stores only the public
  half. Grants are scoped, ledgered and revocable, except the recovery grant.
  Day-to-day keys have mandatory expiry. The recovery key is the startup gate
  plus `key-manage` only. The admin check generalizes
  `evaluate_toy_grant_for_call`: the subject is a Child or an AdminKey.
- **Why:** a uid check is one permanent all-powerful credential, and anyone who
  can read a secret file on Mother's disk holds it.
- **Rejected:** Mother generating the admin secret; an owner-only secret file at
  init; a one-time enrollment token printed to stdout. Reasons in
  [design/admin-keys.md](design/admin-keys.md#1-bootstrap).

### D-08 (b) Iroh is the only admin transport

- **Date:** 2026-10-14.
- **Status:** **decided, not built.** The UDS control socket, the `peer_cred`
  owner check, `open_local` and the `execute_offline_*` commands are still on
  `main`.
- **Decision:** no UDS admin, no uid or `peer_cred` check, no `open_local`.
  `execute_offline_*` is removed (D-15).
- **Why:** same-uid and localhost are not identities. A second auth path is a
  bypass.

### D-09 (c) Mother is essential

- **Date:** 2026-10-14.
- **Status:** decided.
- **Decision:** a Child refuses to act without Mother. Mother unreachable is a
  hard stop for a Child's work, not failover. The effort goes into Mother never
  dying (hot standby, replicated state, fast restart). No-failover covers a
  Child's work, not admin recovery.
- **Why:** a Child that keeps working without its authority is a Child acting on
  stale or absent authority.
- **Rejected:** failover of a Child's work to another Mother or a peer
  ("uncle/aunt"; see D-31).

### D-10 (d) A Child is reached only through a platform-given handle

- **Date:** 2026-10-14.
- **Status:** decided; partly built. Peer calls use iroh. Local `POST /calls`
  still uses the UDS peer-credential preflight.
- **Decision:** a Child is a durable object. No local socket, no same-uid path.
  Holding the handle is the authorization. Local calls move to the iroh call
  path.

### D-11 (e) Wake check-in

- **Date:** 2026-10-14.
- **Status:** decided, not built (build plan phase 5).
- **Decision:** on wake, a Child checks in with Mother before it touches its own
  state. The handle proves identity; the check-in proves it is still her Child
  and that policy is current (revision and generation).

### D-12 (f) Checkpoints

- **Date:** 2026-10-14.
- **Status:** decided, not built (build plan phase 6).
- **Decision:** a Child continues its current task until it needs another.
  Mother interrupts by an iroh message, checked at step checkpoints. If Mother
  drops, the Child finishes the current step and freezes at the next
  checkpoint.

## 3. Admin keys in detail

### D-13 Two bootstrap shapes

- **Date:** 2026-10-14.
- **Status:** decided; pairing-ticket ledger facts built (#48), redeem over iroh not built.
- **Decision:** either a pre-seeded admin public key in deploy config
  (`authorized_keys` style) or a single-use, short-lived iroh pairing ticket,
  redeemed over iroh and burned in the ledger (hash only). Both sides confirm an
  Ed25519 + ML-DSA-65 fingerprint.
- **Rejected:** owner-only secret file at init; printed enrollment token.

### D-14 Route scopes, expiry, and no resurrection

- **Date:** 2026-10-14.
- **Status:** built (#48, #51, #64).
- **Decision:** scopes are the existing route and command names (`status`,
  `approve-child`, `peer-add`, `key-manage`, …). `key-manage` is never implied.
  A deny mints no token, and an allow is not a session token. A revoked admin
  grant keeps its row and cannot be put again (the watch-scope rule, not the
  Toy-grant rule). A key cannot revoke itself. A presented generation other than
  current denies.
- **Why:** an empty scope list, a missing expiry, or "all routes" recreates the
  uid check.
- **Source:** [design/admin-keys.md](design/admin-keys.md#2-scoped-grants), PR #51, PR #64.

### D-15 Hard cutover, no dual-auth window

- **Date:** 2026-10-14.
- **Status:** **decided, not built** (build plan phase 4).
- **Decision:** the old admin path (UDS socket, `peer_cred`, `open_local`,
  `os-uid` principals, `execute_offline_*`) is deleted in the same build that
  adds the iroh admin-key path. No period where both work.
- **Why:** MCT is not live, so there is nothing to migrate. A transition window
  is a second path.
- **Rejected:** "key or uid" and "key and uid" windows; keeping offline commands
  behind a check.
- **Source:** [design/admin-keys.md](design/admin-keys.md#migration).

### D-16 Losing the recovery key is a lockout

- **Date:** 2026-10-14 to 2026-10-15 (exact day not recorded).
- **Status:** historical contract, built (#48, #51); D-54 explicitly amends the
  absolute no-replacement rule for enrolled Voice-bound guardian recovery.
  Until that path is built, the existing runtime still enforces lockout.
- **Decision:** a day-to-day key cannot revoke or replace the recovery key. If
  the recovery secret is lost, that Mother stays unadministrable for the rest of
  the epoch. Wiping the ledger makes a new Mother; it is not a way back in.
- **Why:** MCT has no second principal yet. iroh has no recovery concept. Any
  shortcut is a bypass around `OperatorGatedNonvirgin`.
- **Rejected for now:** Signal-style delayed reset; guardian quorum (both D-41).

### D-17 Admin facts and hybrid signatures: scheme v1

> Historical hybrid contract, preserved for replay/reference. D-58 supersedes
> its mandatory hybrid requirement for new passkey-rooted administration.

- **Date:** 2026-10-15 (decisions); 2026-10-17 (#67 merged).
- **Status:** built, with signatures **optional** on replay.
- **Decision:**
  1. ML-DSA-65 is load-bearing. It is not dropped, deferred, or optional as a
     design goal.
  2. The ML-DSA-65 public key sits on the admin grant fact next to the Ed25519
     key. Both rotate together in one ledger write.
  3. The size and time cost is accepted (measured below).
  4. Transport stays plain Ed25519. iroh never sees the ML-DSA half.
  5. Known gap: a FROST guardian quorum would not cover the ML-DSA half.
  6. The signature covers the command plus the grant under both keys, with
     context `mct-admin-sig/ed25519+mldsa65/v1`. The pairing ticket carries an
     expiry window and the peer's endpoint ID.
- **Why:** post-quantum safety for the authority record; ML-DSA-65 is also what
  makes patina-mct distinct.
- **Measured (2026-10-17, x86_64 KVM, AVX2):** sign p50 114–187 µs, verify p50
  87 µs; hybrid signature 3374 B; five-fact admin session 55.06 KB vs 12.7 KB
  unsigned. aarch64 not measured.
- **Interim:** unsigned admin facts are still accepted on replay, and a grant
  put may be self-signed (proves possession, not authorization). Making
  signatures required is open.
- **Source:** [design/admin-keys.md](design/admin-keys.md#decisions-october-15-2026), PR #67.

### D-18 Ed25519 as the classical half, not P-256

- **Date:** 2026-10-15 (with D-17).
- **Status:** decided.
- **Decision:** the classical half of the hybrid is Ed25519.
- **Why:** admin keys are iroh identities, and iroh endpoint IDs are Ed25519, so
  one classical key serves both transport and signing and the transport check
  stays one verification. The original chat reasoning is not written anywhere
  else; this is the reason as the notes support it.
- **Rejected:** P-256 as the primary key. P-256 is what common hardware
  keystores support, which is why a P-256 hardware wrapper stays a future idea
  (D-42).

### D-19 libcrux-ml-dsa as the ML-DSA implementation

- **Date:** 2026-10-17 (#67).
- **Status:** built.
- **Decision:** `libcrux-ml-dsa =0.0.11`, default features off, `std` +
  `mldsa65`, `simd256` on x86_64 and `simd128` on aarch64. Deterministic FIPS
  204 signing. RustCrypto `ml-dsa =0.1.1` is a dev-dependency only, for a test
  that checks byte-identical keys and signatures over 8 seeds.
- **Why:** recorded in #67 as the measured signer; the full comparison of
  implementations was benchmarked outside the repo and is not committed.
- **Open:** NEON (`simd128`) on Apple silicon is not measured.

### D-20 RUSTSEC-2026-0173 ignored until 2027-04-14

- **Date:** dropped 2026-10-15 (#65, after #61 removed `proc-macro-error2`);
  re-added 2026-10-17 with libcrux (#67).
- **Status:** built (`.cargo/audit.toml`, `osv-scanner.toml`).
- **Decision:** ignore `proc-macro-error2` (unmaintained). It arrives through
  `libcrux-ml-dsa → core-models → hax-lib-macros` only under `cfg(hax)` and is
  never compiled in our builds. Revisit by 2027-04-14, or earlier when libcrux
  or hax drops it.
- **Documentation:** the design-hardening branch reconciles `SECURITY.md`
  with both recorded exceptions; no audit policy is weakened.

## 4. Children, Toys and handles

### D-21 Mother mints every borrowed handle (the courier rule)

- **Date:** 2026-10-14.
- **Status:** decided, not built (build plan phase 7).
- **Decision:** a Child is a courier, never a grantor. Only an older Child may
  ask Mother to share a handle it already holds, or a narrower one. Mother
  mints the narrowed handle and ledgers the delegation. Revoking the original
  takes every borrowed copy.
- **Rejected:** a Child minting or lending a handle itself.

### D-22 Older-child status is a grant; groups hold separate handles

- **Date:** 2026-10-14.
- **Status:** decided, not built.
- **Decision:** older-child status is a ledgered grant scoped to a task. A
  young Child handing a Toy up is a request; Mother reissues it as the older
  Child's own handle. The older Child holds one handle per Toy, never one
  combined handle. A parent task's decision history carried down is a record
  only and never adds power.
- **Why:** a combined handle breaks attenuation.

### D-23 "Do this next" is the interrupt payload; a fix is a grant

- **Date:** 2026-10-14.
- **Status:** decided, not built (phase 10).
- **Decision:** Mother never reaches into or rewrites a Child's state. "Do this
  next" is the interrupt carrying the next task. A fix is a fix Toy, a corrected
  input, or a restart with new parameters, each ledgered.

### D-24 A frozen Child resuming counts as a wake

- **Date:** 2026-10-14.
- **Status:** decided, not built (phase 5).
- **Decision:** resume reuses the (e) check-in. If Mother is unreachable or the
  check-in fails, the Child stays frozen. The ledger records the freeze and the
  check-in.

### D-25 One pause: check, then weigh, then act

- **Date:** 2026-10-14.
- **Status:** decided as the build plan's decision 3; the admin-keys note
  words it as "the proposal's ruling" inside the decision-model section.
  Not built (phase 6).
- **Decision:** the interrupt check and any weighing are one pause. Every Child
  reaches it. No live Mother means no pass: the Child freezes. The grace window
  before freezing is a ledgered policy Mother sets per Child or task, starts at
  the next checkpoint, and is waiting, not permission.

### D-26 Decision-model Toys behind a thin gateway

- **Date:** 2026-10-14.
- **Status:** **proposed** in the admin-keys note; listed as decision 5 and
  phase 8 in the build plan. Treat as proposed until the owner confirms.
- **Decision:** a catalog of decision-model Toys (Jev, Nimble, Local) with one
  WIT interface. Mother holds each credential and grants a fresh per-task
  handle. The gateway only holds the credential and forwards. The model advises;
  it never decides or mints.
- **Rejected:** a Child calling the hosted model with the credential in hand; a
  gateway that enforces policy itself.

### D-27 Replay Toy beside `wasi:random`

- **Date:** 2026-10-14.
- **Status:** placement decided; the Toy itself proposed (phase 9).
- **Decision:** deterministic randomness is its own seeded Toy. `wasi:random`
  stays real entropy. A replayed run reuses the recorded model answer instead
  of calling the model again.
- **Rejected:** seeding the secure random source; using `wasi:random/insecure`
  (no seed, no counter).

### D-28 Pi Durable as a Child: three rulings

- **Date:** 2026-10-14.
- **Status:** ruled in the design note; not built.
- **Decision:** per-tool `replay: "safe"` collapses into the one replay path;
  self-wake is a Toy request that Mother fires; host-owned session storage
  becomes ledger-backed, grant-gated Toy calls.
- **Source:** [design/pi-durable-child.md](design/pi-durable-child.md). Four
  other conflicts stay open ([OPEN-QUESTIONS.md](OPEN-QUESTIONS.md)).

### D-29 Clock Toy beside `wasi:clocks`

- **Date:** 2026-10-14.
- **Status:** proposed.
- **Decision:** recorded, resumable time is a Clock Toy. `wasi:clocks` stays the
  standard import with no MCT override.

### D-30 Durable, not fast: resume is a full re-run

- **Date:** 2026-10-14 (snapshot spike in patina-wasi-pi).
- **Status:** accepted finding.
- **Decision:** wasmtime cannot snapshot a running Child, so a resumed Child
  re-executes from the top, with recorded Toy results coming back from the
  ledger. Open plus resume stayed about 136–165 ms either way; snapshots cut
  storage, not time.

### D-31 Use the older Child; no "uncle/aunt"

- **Date:** 2026-10-14 (call).
- **Status:** decided by consequence of (c) and (d).
- **Decision:** a lateral relative that can grant, wake, or keep a Child going
  when Mother is down is a second grantor. Use the older Child for courier and
  subagent work.
- **Pending:** four speed levers from that call are not accepted
  ([design/pi-durable-handoff.md](design/pi-durable-handoff.md)).

## 5. Restart, recovery and the peer wire

### D-32 Restart recovery settles; it never resumes

- **Date:** 2026-10-17 (#66).
- **Status:** built.
- **Decision:** at startup, under the exclusive writer lock, each leftover
  reservation or run is settled once: completed from its result, closed as
  denied, released if there is no run and no effect evidence, or marked
  interrupted (outcome unknown) if it may have reached a Child or peer. Each
  disposition is a ledger fact with a deterministic ID, appended before the
  state change. Mother never resumes a call or inspects Child state.
- **Rejected:** re-running interrupted calls.

### D-33 `execution_interrupted` call reason

- **Date:** 2026-10-17 (#66).
- **Status:** built.
- **Decision:** a new `CallProtocolReason::ExecutionInterrupted`. Same-key
  retries of an interrupted call replay that reply. No protocol version bump.

### D-34 Torn-tail recovery keeps the evidence

- **Date:** 2026-10-17 (#66).
- **Status:** built.
- **Decision:** a torn final frame is kept as private forensics and one
  tail-recovery fact is appended. Authority projection publish and proof happen
  in one atomic writer step.

### D-35 Operator gate over a recovery-only ledger (D-H3.5)

- **Date:** 2026-10-17 (#68).
- **Status:** built.
- **Decision:** an operator-gated epoch is legal over an empty ledger or over a
  ledger whose every entry is a strictly valid tail-recovery fact with matching
  private forensics. Anything else refuses. Enforced at the gate, at tenure
  start under the writer lock, and on replay.
- **Why:** a crash on the very first ledger write left Mother unable to ever
  start (a fail-closed dead end).
- **Source:** [`ledger-commit-recovery/SPEC.md`](../layer/surface/build/feat/ledger-commit-recovery/SPEC.md),
  product map invariant `OperatorGateAdmitsOnlyRecoveryEvidence`.

### D-36 Peer replies carry at most three reasons

- **Date:** 2026-10-17 (#69).
- **Status:** built.
- **Decision:** `mct/call/0` replies may carry `execution_failed`,
  `execution_timed_out` or `execution_interrupted`, only on failed or timed-out
  replies. Denial, authority, binding, routing, payload and idempotency reasons
  are never sent to peers. Unknown values decode as `Unrecognized` (bounded to
  64 bytes) and are recorded as the literal `unrecognized`. A `success` with any
  reason is rejected. No protocol version bump: the change is additive both
  ways.
- **Why:** keep the concealment rule while letting a forwarding Mother tell a
  restart from a generic failure.

### D-37 Single-hop forwarding; signed bindings

- **Date:** 2026-07-09 to 2026-07-10 (#18, #19).
- **Status:** built.
- **Decision:** one authorized call goes to one admitted peer. A forwarded
  arrival is terminal and cannot be forwarded again. Remote admission needs a
  signed binding, scoped ALPNs, a fresh published surface and a matching
  Vision. A connection alone earns no trust.
- **Source:** [`multi-mother-route-forwarding/`](../layer/surface/build/feat/multi-mother-route-forwarding/), [`mct-peer-ontology.allium`](../layer/allium/mct-peer-ontology.allium).

### D-38 Cluster/replica Mothers remain future scale work

- **Date:** 2026-10-14 (#46), restored 2026-10-15 (#59) after #36 reverted the wording.
- **Status:** future, not planned now.
- **Decision:** clustering, sharding, standby or replica Mothers, guardian
  quorums and shared ledgers are not current scope. Only single-hop peer
  federation is real code (`forwarding.rs`, `candidates.rs`,
  `publication.rs`, `crates/mct-daemon/src/federation.rs`, `mct-iroh`).
- **2026-10-19 scope clarification (D-52):** finishing direct federation across
  4–6 independent Mothers is now a release target. The exclusions above still
  apply; independent per-Mother ledgers do not imply a shared/replicated ledger.
- **2026-10-19 design clarification (D-53):** guardian recovery is now under
  active completion design, not an indefinitely deferred topic. Other scale
  exclusions remain; a guardian runtime contract is not yet ratified.

### D-39 No allow-by-default egress

- **Date:** 2026-07-09 (patinaToy audit).
- **Status:** decided; the network Toy is not built.
- **Decision:** the network Toy is HTTPS only with domain and method allowlists,
  size and time limits, and secret references through the secrets authority.
  Unmatched hosts deny.
- **Rejected:** porting `patinaToy` HTTP, where unmatched hosts fall through to
  default outgoing HTTP.
- **Source:** [`MCT-NEXT-BUILD-TODO.md`](../layer/surface/build/product/MCT-NEXT-BUILD-TODO.md).

### D-40 Child-start ledger fact: option D

- **Date:** 2026-10-18.
- **Status:** **proposed.**
- **Decision:** the ledger writer itself admits a local Child launch and
  appends a `ChildInvoked` BeforeEffect fact before launch; a writer-level guard
  refuses a second admission of the same `call_id`; recovery becomes
  ledger-first.
- **Rejected in the note:** start fact on the caller thread (A1 loses the
  ordering proof, A2 deadlocks); inferring the start from the route fact (C);
  SQLite `synchronous = FULL` as the fix.
- **Source:** [design/child-start-ledger-fact.md](design/child-start-ledger-fact.md); open questions in [OPEN-QUESTIONS.md](OPEN-QUESTIONS.md).

## 6. Future paths (named so they are not mistaken for scope)

### D-41 Recovery through a quorum of Mothers

- **Status:** originally future; D-53 brought this into active design and D-54
  ratifies bounded Voice-bound guardian recovery for this release. Not built.
  The historical paths below are alternatives, not the D-54 implementation.
- **Paths:** (1) Signal-style delayed visible reset: about seven days, every
  admin device warned, cancellable. (2) Argent-style guardian quorum of Mothers.
  FROST is classical-only and there is no threshold ML-DSA, so a FROST quorum
  would not cover the ML-DSA half.

### D-42 P-256 hardware wrapper

- **Status:** future idea, not designed. Source: owner chat only.
- **Idea:** keep the admin secret behind a hardware keystore that only speaks
  P-256 (for example the Secure Enclave), wrapping rather than replacing the
  Ed25519 + ML-DSA-65 key.

### D-43 Ideas from prior art

- **Status:** not decided. Listed in [PRIOR-ART.md](PRIOR-ART.md#ideas-worth-borrowing-not-decided).

## 7. How the project is run

### D-44 Merge commits; merge `main` in; never rebase pushed work

- **Date:** standing since #36 (2026-10-14).
- **Status:** decided.
- **Decision:** PRs merge with a merge commit. No squash, no rebase. When `main`
  moves, merge it into the branch. No force-push. Branches are not deleted after
  merge without the owner's approval.
- **Why:** session and provenance history in `layer/` lives in individual
  commits.

### D-45 Only the owner is credited

- **Date:** 2026-10-15 (#63).
- **Status:** built.
- **Decision:** `scripts/check-authors.sh` (job `authors`) fails any commit
  whose author or committer is not allowlisted, and any `Co-authored-by`
  trailer. GitHub web-flow merge commits are allowed.

### D-46 Branch protection on `main`

- **Date:** in force as of 2026-10-18.
- **Status:** built.
- **Decision:** required checks `fmt`, `clippy`, `test`, `authors`; branch must
  be up to date; admins included; no force-push; no deletion. Checks are never
  bypassed or weakened to land a change.

### D-47 Dependency updates by hand, one coherent PR per group

- **Date:** 2026-10-14 (#56).
- **Status:** decided.
- **Decision:** Dependabot is removed. Updates come as one coherent PR per
  group, with audit, fmt, clippy and tests run first. iroh is pinned (`=1.0.3`)
  and moves only in its own PR.
- **Why:** grouped Dependabot batches mixed wasmtime majors, broke the daemon,
  and would have overridden the iroh pin.

### D-48 Audit policy: every ignore has a reason and a revisit

- **Date:** R2 (2026-07); osv-scanner added 2026-10-15 (#61).
- **Status:** built.
- **Decision:** `cargo audit` fails on every vulnerability and non-exempt
  warning. Each ignore carries a reason and a revisit date or condition,
  checked nightly. `osv-scanner` scans both lockfiles because some advisories
  are not in RustSec.

### D-49 OpenSSF Scorecard removed

- **Date:** 2026-10-15 (#60, #62).
- **Status:** built.
- **Decision:** the repo is private, so Scorecard cannot publish results. The
  workflow is gone.

### D-50 Allium retired from the active development path

- **Date:** 2026-10-18.
- **Status:** built.
- **Decision:** Allium is no longer semantic law, a required development
  phase, or a CI dependency. Existing files under `layer/allium/` are frozen
  in place as historical evidence so prior sessions, audits, and commits keep
  valid references. Settled decisions, active ratified slice specifications,
  standing core doctrine, and executable tests now carry the active product
  contract. Restoring Allium requires a new explicit governance decision.
- **Why:** the repository now has a durable decision log, explicit built versus
  proposed status, bounded slice specifications, and extensive executable
  proof. The Allium checker remained pre-release, did not evaluate much of the
  prose contract corpus, and returned no findings over the large MCT maps;
  mandatory Allium workflows added ceremony without enough unique signal.
- **Rejected:** immediate deletion of the maps and history. Git and the frozen
  files preserve the prior model while removing it from the critical path.
- **Source:** owner decision following the PR #70 design-history review.

### D-51 Caller-scoped durable dispatch fence (review branch)

- **Status:** proposed contract, implemented on the design-hardening branch for
  owner review; not a claim that current releases contain it.
- **Scope:** preserve existing keyed/unkeyed and caller-scope semantics while
  preventing lost SQLite history from authorizing a second keyed dispatch
  within its 720-second window. A durable reservation precedes entry to the
  existing authority pipeline. If the result projection is lost, refuse with
  `execution_interrupted` rather than guessing or re-executing.
- **Tradeoff:** one additional synced fact per fresh keyed dispatch; a crash
  before actual launch may conservatively interrupt work that never ran.
- **Boundary:** this does not grant authority, prove Child launch, reconstruct
  result bytes, or implement D-30 task resume. D-40 remains proposed. No
  signature, recovery-key, or portable-handle policy changes are implied.
- **Source:** owner request to implement the architecture review on a branch;
  exact choices and tests in
  [`ledger-first-call-recovery/SPEC.md`](../layer/surface/build/feat/ledger-first-call-recovery/SPEC.md).

### D-52 Finish independent Mothers and bounded federation together

- **Date:** 2026-10-19.
- **Status:** owner-directed completion scope; implementation in progress on
  `feat/finish-mct-federation`, not a released capability claim.
- **Decision:** finish single-node administration, canonical authority,
  execution/recovery, and operations alongside direct cooperation of 4–6
  independent Mothers in one explicitly agreed Vision. Each Mother owns its
  identity, state, ledger, and local authorization. Unavailable peers must not
  stop unrelated local work. No shared global policy-revision counter.
- **Boundary:** D-09 no-failover and D-37 single-hop remain. No takeover,
  clustering, guardian quorum, shared ledger, or whole-Pi port prerequisite.
  Existing one-local-Vision constraints remain for this bounded release.
  Unsettled admin issuance, key compromise, and final admission contracts are
  explicit blockers, not implicitly ratified by this completion direction.
- **Source:** owner accepted the 4–6-Mother finish direction and requested a
  new branch/clone and executable check. [FINISH.md](FINISH.md) tracks all exit
  obligations; the first bounded implementation contract is
  [federation-finish/SPEC.md](../layer/surface/build/feat/federation-finish/SPEC.md).
- **Subsequent owner direction (D-53):** reconsider the guardian-recovery
  exclusion during this completion effort. This authorizes active design;
  protocol and release-obligation amendments must be explicit before building.

### D-53 Bring guardian recovery into active multi-Mother completion design

- **Date:** 2026-10-19.
- **Status:** owner-directed design scope; concrete policy proposed, not built.
- **Decision:** investigate and shape guardian recovery now alongside finishing
  multiple Mothers, rather than parking it at an indefinite future milestone.
  Lean on researched Signal, Iroh, and Argent/Argent X patterns. The owner asks
  the assistant to recommend the concrete design.
- **Boundary:** this revises D-41's deferral and D-52's exclusion for design
  consideration. It does not ratify a quorum threshold, delay, cancellation
  rule, runtime root replacement, or restoration of a destroyed/compromised
  host. D-16 remains the runtime contract until explicitly amended. No work
  takeover, shared ledger, implicit guardian rights for peers, or production
  deployment is authorized.
- **Recommendation under review:** recovery-only guardians for an intact,
  trusted Mother, locally committed recovery transitions, explicit prior
  appointment, and separate hybrid approvals. Target inclusion in this effort
  through a bounded slice, not by claiming federation tests prove recovery.
- **Source:** owner discussion following Signal/Iroh and Argent research;
  [guardian-recovery-v1.md](design/guardian-recovery-v1.md),
  [guardian-recovery-research.md](design/guardian-recovery-research.md), and
  [recovery-signal-iroh-research.md](design/recovery-signal-iroh-research.md).
- **Follow-through:** D-54 ratifies the concrete first-gate contract following
  the Voice history review; D-53's pending-policy wording above is historical.

### D-54 / D-55 / D-56 / D-57 — archived guardian/hybrid plan

Superseded for new administration by D-58. Original decisions, source checkpoint,
and spec/test locations are preserved in
[the archive](archive/2026-10-28-guardian-hybrid-admin-plan.md).
Historical hybrid signatures and replay contracts remain supported, not promoted
into the new passkey profile.

### D-58 Passkey-rooted, 24-hour Ed25519 administration

- **Owner decision:** 2026-10-28; replaces the guardian/hybrid ceremony for new administration.
- **Root:** a separately registered WebAuthn P-256 credential, bound to its RP ID,
  origin, Mother and ledger. A self-presented key or unsigned legacy root cannot
  establish provenance. Registration/trusted setup is distinct from login.
- **Day keys:** passkey-issued Ed25519 leases, explicit scopes and mandatory expiry
  no more than 24 hours after Mother issuance. No ML-DSA half is required.
- **Commands:** verify possession, exact target/snapshot, scope, Mother time and
  nonce; persist admission before effect. Expiry and revocation are rechecked.
- **Revocation:** the passkey root or an active KeyManage day key may immediately
  revoke any DayToDay grant, including itself; unrelated grants remain active.
  Day keys cannot issue other day keys, replace roots, or extend their leases.
- **Compatibility:** preserve old code/tests and facts without auto-migration or
  permitting legacy roots to mint new-profile authority.
- **Proof:** deterministic automated tests and repository quality gates. No new
  temporary receipt-hash tables or long evidence essays.
