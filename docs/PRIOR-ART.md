# Prior art

How other systems separate authority from execution, and how MCT compares.
Research done 2026-10-18 from public sources. Vendor figures were not
reproduced. Anything marked **unverified** was not confirmed from a primary
source.

## Short answer

- MCT's core ideas are not new. No ambient authority, host-side checks on every
  call, narrowing capabilities, and replay of journaled results are classic
  object-capability design. Golem, Cloudflare's bindings, Microsoft's Wassette,
  Agoric and Sandstorm all do versions of it.
- We found **no system that combines all four** of: durable replay, a single
  authority that alone admits work, a signed hash-chained ledger as the source
  of truth, and capabilities granted per worker. Each piece exists somewhere.
  A closed or unpublished system could still exist.
- What looks distinctive in MCT: a separate Mother that holds every key (Mother
  unreachable is a hard stop), and an authority ledger that is hash-chained and
  carries post-quantum hybrid signatures on admin facts.
- We found no pattern clearly cheaper than an in-process WASM Child under a
  host that checks each call.

## Approaches

### 1. Runtime-held capabilities: Golem 1.6

Durable WebAssembly agent runtime on wasmtime. Each agent, and each tool call,
gets its own instance. Every file, network, secret, tool and agent-to-agent
call goes through the host, which checks the agent's permission cards. Cards
can only be derived narrower, can expire, and revoking one revokes everything
derived from it. An upper bound per agent rules things out even if a later card
would allow them. Golem records which cards each call used, so replay makes
the same decisions. Secrets are opaque handles; revealing a value needs its own
permission and is recorded. Oplog writes are fenced by an epoch so a server
that lost ownership cannot keep appending. Vendor figures: about 2 ms cold
start, about 1 MB per agent (**unverified**).

- **Versus MCT:** almost the same model, with the runtime in Mother's place.
  The authority is the runtime, not a separate party. Its log is an operations
  log; its docs show no hash chain or signatures (**unverified**: the source
  was not read).
- Sources: [permissions](https://learn.golem.cloud/v1.6/concepts/permissions), [persistence](https://learn.golem.cloud/v1.6/operate/persistence).

### 2. Bindings instead of network: Cloudflare Code Mode, Pi Durable codemode

Model-written code runs in an isolate with `fetch` and outbound connections
off. Its only reach is bindings that the supervisor fulfils while holding the
keys. Cloudflare's durable runtime keeps a per-call log in SQLite, marks a call
as executing before it runs, and supports approval pauses, replay, divergence
detection and rollback; results over 1 MB are capped and passed by reference.
Pi Durable's codemode does the same in QuickJS with a virtual clock, seeded
randomness, and results journaled before they reach the code. Isolates start in
milliseconds.

- **Versus MCT:** the same idea as Mother handing a Child its handles. The
  supervisor and the runtime are one party, and the log is unsigned. Cloudflare
  re-runs a call that was executing at a crash; MCT's proposed once-only guard
  refuses it (fails closed), which is stricter.
- Sources: [Code Mode post](https://blog.cloudflare.com/code-mode/), [Code Mode runtime](https://cdn.jsdelivr.net/npm/@cloudflare/codemode@0.5.1/docs/runtime.md), [Pi Durable](https://github.com/earendil-works/pi/blob/main/packages/durable/README.md).

### 3. Environment sandboxes: Claude Code, OpenAI Codex, Firecracker and E2B, PiD runner VM

Claude Code uses bubblewrap or macOS Seatbelt plus a domain-filtering proxy.
Codex uses bubblewrap and seccomp with networking off by default. E2B and
Firecracker use microVMs. The owner's PiD service runs in a runner VM. These
limit what the agent can touch, not which calls it may make. Firecracker boots
in about 125 ms raw, about 1.1 s to fully ready; E2B reported a 586 ms median
allocation at 100,000 concurrent sandboxes. Anthropic reports 84% fewer
permission prompts with sandboxing.

- **Versus MCT:** heavier and coarser. MCT decides per call; these fence the
  environment. Their payoff is fewer prompts, not finer authority.
- Source: [Claude Code sandboxing](https://www.anthropic.com/engineering/claude-code-sandboxing).

### 4. Information-flow control: DeepMind CaMeL

A privileged model writes the plan as code. An interpreter tags every value
with where it came from and who may read it, and blocks tool calls that would
leak. 77% of benchmark tasks solved with provable security, against 84% with no
defence. It builds on Simon Willison's "lethal trifecta".

- **Versus MCT:** MCT decides which tools a Child may call, not where a value
  may flow. MCT has nothing like this.
- Sources: [CaMeL](https://arxiv.org/abs/2503.18813), [the lethal trifecta](https://simonwillison.net/2025/jun/16/the-lethal-trifecta/).

### 5. Identity and OAuth scopes: MCP authorization, SPIFFE

Scopes are granted per server, not per tool; per-tool scopes are a draft
proposal. It depends on each server behaving.

- **Versus MCT:** coarser and weaker.
- Source: [MCP authorization](https://modelcontextprotocol.io/specification/2025-11-25/basic/authorization).

### 6. Durable workflow engines: Temporal, Restate

Journal and replay, with an unsigned database or log as the truth. No authority
model of their own.

### 7. Other capability systems

- **Microsoft Wassette:** per-component WASM permissions for MCP tools.
  [Permission system](https://microsoft.github.io/wassette/latest/design/permission-system.html).
- **Kenton Varda: Sandstorm and "Gatekeepers".** Capability-based app
  isolation; the Gatekeepers idea simulates what an action will do before
  approving it, and approves in batches. No link was kept for the post; only
  its opening was read.

## Who combines the four pieces

| System | Durable replay | Single authority | Signed hash-chained ledger as truth | Per-worker capabilities |
| --- | --- | --- | --- | --- |
| **MCT** | authority replay on restart (built); Child resume by re-run with recorded Toy results (designed, D-30) | Mother | hash chain on every fact; hybrid signatures on admin facts, optional for now | ToyGrants per Child |
| [Janus](https://arxiv.org/html/2609.38266v1) (Sept 2026 paper) | log is the only truth; state rebuilt from it | yes | BLAKE3 chain, Ed25519-signed segments, logged before effect | no sandboxed workers; tools through an MCP proxy |
| [Golem 1.6](https://learn.golem.cloud/v1.6/operate/persistence) | yes | the runtime | no chain or signatures found in docs (**unverified**) | permission cards |
| [Agoric SwingSet](https://github.com/Agoric/agoric-sdk/blob/master/packages/SwingSet/docs/delivery.md) | yes; divergence is fatal | kernel, under blockchain validators | store is primary, not a rebuildable cache | per-vat c-lists |
| [Microsoft CCF](https://ccf.dev/main/architecture/ledger.html) | no workflow replay | consortium governance | signed Merkle ledger; state rebuilt from it | role-based, not capabilities |
| [Authority at Commit Time](https://arxiv.org/abs/2609.31490) (Salas) | at-most-once effects | one authority commits each effect; pauses when unreachable | SQLite is the truth | not the focus |
| [Agent Flight Recorder](https://arxiv.org/abs/2609.01931) | listed in the research; not summarized here (**unverified**) | | | |

## Why nobody builds the full stack

- **Cost per call.** Signing and fsyncing before every action gets expensive
  under load. Janus adds a median 5.4 ms per gated step and misses its budget
  at 256 concurrent steps (the paper flags its own numbers as shaky). MCT's
  measured extra synced frame is about 76 µs on a Linux VM, mostly because it is
  one machine, one writer, and admin facts (not every call) are signed. Mac cost
  is not measured.
- **Availability.** A single authority means a hard stop when it is down. Most
  frameworks pick a database that stays up, or pay for consensus.
- **Key management and demand.** Signed agent logs only appear in 2025–26
  research prototypes. Janus cites the EU DORA regulation as motivation.

What people use instead: an unsigned database or log as the truth (Temporal,
Restate, Golem, Cloudflare); publishing log hashes to a blockchain or
transparency log; capability tokens with no ledger; or environment sandboxes
plus ordinary audit logs.

## Ideas worth borrowing (not decided)

None of these is a decision. Each needs its own proposal.

- From Golem: grants that can only narrow, with expiry, cascading revoke and an
  upper bound per Child; record in the ledger which grant each call used, so
  replay re-checks against the same decision. Golem's epoch-fenced writes
  support MCT's epoch design.
- From Cloudflare: mark a call executing before it runs (this is the proposed
  `ChildInvoked` fact); a flag to re-run large idempotent reads rather than
  journal them; a 1 MB result cap with pass-by-reference; hand replay divergence
  back to the agent as data; optional revert hooks.
- From Gatekeepers: simulate an action before approving it; approve in batches.
- From Janus: segment-level signing and group commit, if per-call signing ever
  becomes too expensive.
- From CaMeL, later and only if prompt injection becomes a concern: tags on
  where data came from.
