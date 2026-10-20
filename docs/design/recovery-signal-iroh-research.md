# Recovery research: Signal and Iroh

**Status: research, not an owner decision or implementation contract.**
Retrieved 2026-10-19. Supports [[finish-owner-gates]] and the recovery discussion
in [[admin-keys]]. D-16 remains unchanged; no owner gate is approved by this note.

## Findings

### Signal separates registration, data recovery, and cryptographic trust

1. **Lost/stolen phone:** Signal's support page says access to the phone number
   is needed. Registering a replacement phone stops messaging/calling on the old
   phone, does not erase its past history, and gives contacts a new safety number
   in this recovery scenario. This is service-mediated replacement, not recovery
   of an irreplaceable signing secret. [S1]
2. **PIN:** recovers profile, settings, contacts, and block list; it is not a chat
   backup key, SMS code, or device unlock code. Signal says it cannot recover or
   reset the PIN. Registration Lock expires after **seven days of inactivity**;
   a new PIN can then be created, but the old PIN and information associated with
   it are no longer available. This is not an unconditional seven-day recovery
   timer that defeats an active attacker. [S2]
3. **Backup recovery key:** the optional Secure Backup archive has a separate
   64-character key not shared with the Signal service. Without that key Signal
   cannot decrypt or restore the archive. Account re-registration and archive
   decryption are different capabilities. [S3]
4. **Trust change:** safety-number changes are surfaced. If the number was marked
   verified, a change requires manual approval before another message is sent.
   Not every reinstall/device change necessarily changes the safety number.
   This verifies the cryptographic relationship, not a person's real-world
   identity by itself. [S4]
5. **Linked devices:** current support requires the registered primary device to
   relink an unlinked device. This supports a primary/linked distinction, not a
   blanket conclusion about every possible primary-device authority. [S5]

The 2019 Secure Value Recovery article explains PIN-protected remote state,
server-side enclaves, bounded guesses, and replicated consensus to prevent
rollback from granting extra guesses. It is a **historical design explanation**,
not a verified description of every current Signal backend component. It does
show why copying a PIN UI or timeout does not copy the recovery system. [S6]

### Iroh separates transport authentication from application authorization

1. An EndpointId is the public half of an Ed25519 keypair. By default a new
   endpoint gets a fresh identity; persisting and reloading the SecretKey keeps
   the EndpointId across restarts. Anyone possessing that secret can impersonate
   the endpoint. The same key must not be used by concurrent endpoints. [I1]
2. Discussion #4221 is **not a prescription for irreversible application
   identity**. Maintainer matheus23 recommends ephemeral transport EndpointIds,
   authenticated above transport by a hardware-backed institutional identity
   signing the endpoint ID and additional authorization/revocation information.
   This is architectural advice for that use case, not a complete recovery or
   rotation protocol. [I2]
3. A later maintainer reply by Frando corrects the discussion's initial claim
   that outgoing hooks are absent: `EndpointHooks::before_connect` can initiate
   a separate authentication ALPN before application traffic. Read the full
   discussion, not only the first exchange. [I2]
4. The repository pins Iroh **1.0.3**. Its tagged `auth-hook.rs` example implements
   `before_connect` and `after_handshake` with a separate auth ALPN. This confirms
   the integration mechanism exists at our pinned version. The example uses a
   shared token and cached endpoint allowlists: it is **not** an MCT-compatible
   grant, expiry, revocation, hybrid-signature, or recovery implementation. [I3]
5. Iroh's FAQ assigns application admission policy to applications and explicitly
   says its transport is not post-quantum-secure. MCT's hybrid admin signatures
   must not be described as making Iroh transport post-quantum-secure. [I4]

## What this establishes for MCT (analysis, not upstream guarantees)

| Failure | Relevant distinction |
| --- | --- |
| Admin device lost, safe recovery-key backup survives | Lost device is not lost credential; restoring the same credential is not granting a new principal. A custody/restore drill still needs specification and proof. |
| Every recovery-key copy lost | Neither Iroh nor Signal can reconstruct arbitrary missing key material. Without another pre-authorized recovery mechanism, D-16 lockout remains. Valid day grants do not automatically disappear. |
| Recovery key stolen | Restoring a backup does not remove the attacker's copy. In-place replacement needs a pre-authorized way to override the stolen credential, absent under D-16. |
| Mother endpoint key lost or stolen | Separate from admin recovery-key loss. Iroh changes EndpointId when its key changes; MCT decides whether and how a new transport key can bind to existing authority. This release does not yet support live identity rotation. |
| Mother host compromised | Key replacement alone cannot establish that execution, storage, or the ledger remain trustworthy. The trusted-host boundary still applies. |

Signal's account service and phone-number verification supply a recovery path
outside the lost device. MCT currently has no equivalent service or approved
second recovery principal. A federation peer is not implicitly a guardian.
A timer by itself cannot determine which holder of a stolen root key is the
legitimate owner or revoke that key across independent peers.

**Provisional recommendation:** distinguish safe backup restoration from total
credential loss and compromise. Do not justify Mother retirement by saying
"Iroh requires it" or "Signal does it." Retirement is a bounded-release
containment policy under MCT's chosen no-replacement rule, not a cryptographic
necessity imposed by the transport. Keeping it, or adding an independent
recovery authority, requires the owner's explicit decision.

## Corrections to earlier shorthand

- `admin-keys.md` describes the current rule as "Signal minus the seven-day
  reset." This obscures the distinction between registration-lock expiry,
  restoration of data, and recovery of signing authority.
- Its description of a future delayed, warned, cancellable reset is an **MCT
  proposal**, not behavior established by the Signal sources inspected here.
- "Iroh has no recovery, rotation, or revocation concept" is too broad if read
  as a restriction on applications. Iroh authenticates transport keys; its
  maintainers explicitly describe building authorization/revocation above them.
- No settled decision or historical design text has been silently rewritten by
  this research. Resolve the owner gate, then record a linked clarification.

## Sources and verification limits

Signal HTML support requests returned HTTP 403. The official public Help Center
JSON API returned the article bodies; those were read directly (not search
snippets). Canonical human-readable URLs are below. Local downloads are in
`/tmp/mct-recovery-research/`; they are temporary, not release evidence.

- **S1:** [What do I do if my phone is lost or stolen?](https://support.signal.org/hc/en-us/articles/360007062452-What-do-I-do-if-my-phone-is-lost-or-stolen)
- **S2:** [Signal PIN](https://support.signal.org/hc/en-us/articles/360007059792-Signal-PIN)
  ([official JSON](https://support.signal.org/api/v2/help_center/en-us/articles/360007059792.json))
- **S3:** [Signal Secure Backups](https://support.signal.org/hc/en-us/articles/9708267671322-Signal-Secure-Backups)
- **S4:** [Safety numbers and changes](https://support.signal.org/hc/en-us/articles/360007060632-What-is-a-safety-number-and-why-do-I-see-that-it-changed)
- **S5:** [Linked Devices](https://support.signal.org/hc/en-us/articles/360007320551-Linked-Devices)
- **S6:** [Technology Preview for secure value recovery](https://signal.org/blog/secure-value-recovery/)
  (2019), and [Introducing Signal PINs](https://signal.org/blog/signal-pins/)
  (2020).
- **I1:** [Endpoints](https://docs.iroh.computer/concepts/endpoints)
  and [Creating an Endpoint: persistent identity](https://docs.iroh.computer/connecting/creating-endpoint#persistent-identity)
- **I2:** [Iroh discussion #4221](https://github.com/n0-computer/iroh/discussions/4221),
  particularly the maintainer replies on May 11, 2026. Later third-party hardware
  validation claims were not independently tested here.
- **I3:** [Iroh v1.0.3 auth-hook example](https://github.com/n0-computer/iroh/blob/v1.0.3/iroh/examples/auth-hook.rs)
  (source inspected, not executed).
- **I4:** [Iroh FAQ](https://docs.iroh.computer/about/faq), especially endpoint
  admission, Ed25519 key types, and post-quantum sections.

Upstream pages contain inconsistencies outside these narrow findings: Signal's
older general restore/troubleshooting prose conflicts with its dedicated Secure
Backups documentation; Iroh's security overview suggests configurable crypto
while the FAQ says endpoint key types are fixed, and its FAQ mixes older TLS
wording with raw-public-key wording. Do not generalize those pages into a full
current implementation audit. Prefer version-pinned source for implementation
claims. No Signal client/server audit, hardware custody experiment, or MCT
runtime recovery test was performed in this research pass.
