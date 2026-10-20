# Voice identity: recovered plans, review sessions, and code

**Status: historical research, not a new authority decision.** Inspected
2026-10-19 for the owner discussion in [[guardian-recovery-v1]].

## Correction

Voice was explicitly an identity concept in the integrated Patina design. The
initial answer based only on the standalone MCT tree was too narrow. The older
source is `/Users/nicabar/Projects/Sandbox/AI/RUST/patina`, at
`d8f90270a53047b99d12004f834b62dbc629570d` plus unrelated local changes. It was
inspected read-only; none of its local changes were modified.

Use comparative vocabulary: those historical sources describe `patinaMother`.
They are prior art for `mctMother`, not automatically inherited runtime law.

## Timeline and evidence

### March 20–21: identity distinct from machine

`layer/sessions/20260320-212325-011658000.md:151-167` records a concrete example:
Bob has two machines and two personas; Alice shares persona C. The model was:
Mother = machine node, persona = cryptographic namespace, project = workspace.
The session records external audit rounds and a handshake field accepted/logged
but not cryptographically enforced in pre-v1.

The final pre-v1 plan at commit `0048e3dc`,
`layer/surface/build/refactor/patina-pre-v1/SPEC.md:369-376`, explicitly says:

- one Mother per machine;
- personas are namespaces within Mother, not separate instances;
- the same persona keypair can span multiple Mothers;
- the connection carries persona context for future scoping.

These appear under **Architecture Must Not Block (post-v1 direction)**, explicitly
not that build's implemented scope. The plan's existence is not proof of crypto
identity, replication, authorization, or recovery.

### April 9: owner explicitly chooses the name Voice

`layer/sessions/20260409-070410-485377000.md:67-76` records completion of the
Era 3 persona-to-voice rename and the vocabulary:

> Mother=WHERE, Project=WHAT, Child=HOW, Toy=CAN, Pando=PRODUCT, Voice=WHO

Its user-prompt index includes "i like voice .. lets hold that as the one" and
the instruction to rename only Era 3 identity, leaving older persona concepts
untouched. Build and audit work is linked to commits `7e544576` and `5a941248`;
release `ed4bcba3` is titled `v0.49.1 — voice-rename`.

The rename SPEC at `8ba1557a`,
`layer/surface/build/refactor/voice-rename/SPEC.md`, distinguishes:

1. Era 1 persona: knowledge notebook/oracle;
2. Era 2 persona: an earlier separate-instance model, subsequently scoped down;
3. Era 3 Voice: cryptographic identity namespace, the WHO behind knowledge.

Its non-goals explicitly exclude crypto keypair generation, voice belief schema,
P2P federation, and any new Voice functionality. Completion of the rename does
not mean completion of the cryptographic design.

### Explicit multi-Mother identity statement

`layer/surface/epistemic/beliefs/voice-keypair-is-node-identity.md:15` says:

> The voice keypair is voice identity ... It is NOT node/machine identity ...
> A voice keypair can exist on multiple Mothers (same voice, multiple machines).
> A Mother hosts multiple voice keypairs (multiple contexts, one machine).

The filename is misleading: the body and March 21 revision explicitly separate
Voice from node identity. The belief is marked active/endorsed, not executable
proof. The scoped predecessor `persona-is-a-patina-instance.md` gives examples
such as Development-Nick, Consultant-Nick, and Client-ABC-Production. A Voice was
therefore a knowledge/context identity, not necessarily one legal human or a
unique human account.

### What the inspected code implements

- `mother/src/protocol.rs:18`: `VoiceUid(pub String)`.
- `mother/src/protocol.rs:72-76`: the `ConnectPayload.voice` comment says it is an
  opaque UID/string in pre-v1, with cryptographic identity/capability verification
  deferred to future phases.
- `mother/src/state/mod.rs:134-146`: VoiceUid validates only a nonempty string.
- `src/commands/ai/surface.rs:677`: resolution is explicit `--voice`, then the
  project `.patina/voice` binding, then none. Legacy path migration is present.
- `src/paths.rs:392-433`: per-Voice directories, `identity.age`, and `beliefs.db`
  paths exist.
- `src/workspace/internal.rs:131-141`: initial setup creates the default Voice
  directory and database, but creates `identity.age` as an **empty placeholder**.

These establish real namespace/session/storage plumbing, not proof of Voice
ownership or a same-Voice security check. This was source inspection, not a test
run or a full audit of the old repository.

## Relation to current MCT and guardian design

The standalone MCT tree has node/endpoint identity, admin public-key grants,
and Vision scope. Its `admin-keys.md` also uses lowercase "voice" for a Child's
risk posture, but that is not the full historical Voice identity model.
Vision is a sharing boundary, not a substitute spelling for Voice.

The owner's recollection is supported: the same Voice across Mothers was
explicitly planned. No guardian eligibility rule requiring the same Voice was
found in the cited plans, review sessions, or implementation. That connection is
now a design question, not an already implemented restriction.

Before finalizing guardian recovery, distinguish:

- recovery of a Mother-wide administrative root;
- recovery of Voice credentials;
- recovery of one Voice's authority binding to a particular Mother.

A same-string VoiceUid must never grant guardian eligibility. Historical shared
Voice-key custody is also not the same construction as the newer proposal for
independent per-Mother credentials: preserving a logical identity while using
separate credentials requires an explicit binding/recovery design. Do not
silently equate them or adopt legacy UCAN/key-sharing proposals as MCT law.

## Stable source links

- [Pre-v1 design boundary](https://github.com/NicabarNimble/patina/blob/0048e3dc/layer/surface/build/refactor/patina-pre-v1/SPEC.md#architecture-must-not-block-post-v1-direction)
- [Rename specification](https://github.com/NicabarNimble/patina/blob/8ba1557a/layer/surface/build/refactor/voice-rename/SPEC.md)
- [April 9 review session](https://github.com/NicabarNimble/patina/blob/d8f90270a53047b99d12004f834b62dbc629570d/layer/sessions/20260409-070410-485377000.md)
- [Voice identity belief](https://github.com/NicabarNimble/patina/blob/d8f90270a53047b99d12004f834b62dbc629570d/layer/surface/epistemic/beliefs/voice-keypair-is-node-identity.md)
- [Protocol implementation](https://github.com/NicabarNimble/patina/blob/d8f90270a53047b99d12004f834b62dbc629570d/mother/src/protocol.rs#L72-L76)

Sources were read locally, including historical Git objects. Links identify the
corresponding repository revisions; no remote availability check was performed.
