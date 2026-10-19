# Guardian recovery: Argent patterns and independent Mothers

**Status: research and future design direction, not ratified implementation.**
Retrieved 2026-10-19. Continues [[recovery-signal-iroh-research]] and
[[finish-owner-gates]]. The owner suggested investigating Argent/Argent X
with another Mother potentially serving as a guardian at some point. This is
not approval of a threshold, a recovery protocol, or expanding this release.
At that point D-16, D-38, D-41, and D-52 remained unchanged.

**Subsequent owner clarification (D-53):** guardian recovery is now active design
within this completion effort, not an indefinite future topic. The deferral
recommendation below records the earlier assessment; it is superseded for
planning by [[guardian-recovery-v1]]. Concrete recovery policy remains proposed.

## Primary-source findings

The former Argent documentation endpoint redirects to Ready documentation.
Its contracts/audits page links the Argent Starknet contracts repository.
Do not treat Ethereum Argent, Starknet accounts, and Starknet multisig recovery
as one identical guardian scheme, or assume all deployed wallets use the latest
repository revision.

### Ethereum Argent contracts

Inspected revision: `0e9bf648ae65e6c5e1d44e0e7cbaa46ff813d878`.

- Guardians are explicitly enrolled. Guardian changes use pending records,
  security delays, and confirmation windows.
- `ArgentModule.getRequiredSignatures` requires `ceil(G / 2)` guardian
  signatures, with the owner disallowed, to initiate recovery. The code calls
  this a majority, but it is **not a strict majority for even G**. Do not copy
  the label without its actual arithmetic.
- Recovery records the replacement owner, execution deadline, and guardian
  count, and locks the wallet. After the delay, anyone can submit finalization;
  they cannot substitute a new owner for the one already authorized.
- Cancellation uses `ceil((G_at_start + 1) / 2)` signatures, with the owner
  optional. It is not an unconditional owner-only veto.
- Finalization replaces the owner and clears the session. The account persists
  as a smart contract whose state transition is enforced by the chain.
- Recovery signatures are separately checked, with duplicate signers denied.
  This is not threshold key reconstruction or a requirement for FROST.

Sources: [SecurityManager.sol](https://github.com/argentlabs/argent-contracts/blob/0e9bf648ae65e6c5e1d44e0e7cbaa46ff813d878/contracts/modules/SecurityManager.sol),
[ArgentModule.sol](https://github.com/argentlabs/argent-contracts/blob/0e9bf648ae65e6c5e1d44e0e7cbaa46ff813d878/contracts/modules/ArgentModule.sol),
[RelayerManager.sol](https://github.com/argentlabs/argent-contracts/blob/0e9bf648ae65e6c5e1d44e0e7cbaa46ff813d878/contracts/modules/RelayerManager.sol).

### Starknet Argent accounts and multisigs

Inspected revision: `c30f61c5b43e12fb88ea64dcdad9649033685d83`.

- The individual account documentation describes optional guardians. With
  guardians, ordinary transactions require one owner AND one guardian; without
  guardians, one owner suffices. This is more than recovery-only guardianship.
- Escape operations permit one role to replace the other after a delay. The
  documented default security period is seven days and is configurable. Owners
  can override guardian-initiated escapes; completion has an expiry window.
- The documentation explicitly warns that it cannot protect the combination
  of ALL owner keys being lost and one owner key being compromised. Owner
  priority is not automatically safe when the attacker is the remaining owner.
- Separate multisig recovery is disabled by default. Owners configure a
  guardian address (which can itself be another multisig), security period,
  and expiration period. The guardian queues a bounded signer/threshold change;
  existing owners with sufficient signatures can cancel it. Anyone may finalize
  after the security delay and before expiry.

Sources: [Argent account](https://github.com/argentlabs/argent-contracts-starknet/blob/c30f61c5b43e12fb88ea64dcdad9649033685d83/docs/argent_account.md),
[escape process](https://github.com/argentlabs/argent-contracts-starknet/blob/c30f61c5b43e12fb88ea64dcdad9649033685d83/docs/argent_account_escape.md),
[multisig recovery](https://github.com/argentlabs/argent-contracts-starknet/blob/c30f61c5b43e12fb88ea64dcdad9649033685d83/docs/multisig_recovery.md),
[Ready contracts and audits](https://docs.ready.co/support-and-ressources/contracts-and-audits).

## Application to MCT (analysis/proposals only)

A Mother could be explicitly appointed a recovery guardian for another Mother.
Sharing a Vision or being an admitted peer would not confer this power. This
would be a new authority role, capable of authorizing a specific recovery
transition under a pre-existing policy, not routine Child execution or general
administration. A quorum able to replace the root is nevertheless collectively
root-equivalent in its eventual power; naming its action narrow does not remove
that trust consequence.

Recovery-only guardians are a better match for independent local operation than
requiring guardian co-signatures on every ordinary request. Guardians being
unavailable could block recovery without blocking unrelated local work.

Separate the failures:

- **Process/host temporarily down:** restart and replay surviving state. Guardian
  approval is not itself a reason to change authority.
- **Admin keys unavailable:** a future guardian policy could authorize replacing
  an administrative root, explicitly amending D-16.
- **Storage or host destroyed:** signatures cannot reconstruct missing identity
  material or ledger bytes. Backups and validated restoration are separate.
- **Host/identity compromised:** guardians cannot remotely make hostile software
  obey revocation. Peers must enforce which restored identity/history they trust.

The major difference from Argent is its chain. The chain provides persistent
account state, transaction ordering, timestamps, and a place to enforce
conflicting recovery/cancellation attempts even while the owner's device is
absent. Iroh provides authenticated transport, **not that shared state machine**.
A proposed MCT protocol must name the enforcement point while Mother is stopped,
the trusted surviving policy/history, and how stale or conflicting recovery
certificates and restored copies are rejected. Do not smuggle in replicated
ledgers, consensus, or takeover by calling them guardianship.

A future quorum could carry separate Ed25519 + ML-DSA signatures from each
appointed guardian over the same bounded recovery statement. Checking k distinct
hybrid approvals does **not** require FROST or threshold ML-DSA. This is a possible
cryptographic construction, not an implemented protocol: canonical policy,
freshness, replay prevention, custody, partitions, and cancellation still need
design and adversarial proof.

A 2-of-3 arrangement is an illustrative availability/security tradeoff, not a
recommendation already accepted. Machines sharing root access, recovery secrets,
backup custody, or an admin credential may not be independent guardians.
A single guardian means that guardian has unilateral recovery authority; a quorum
means enough independent guardians must survive and agree.

## Next decision boundary

Keep guardian recovery as a concrete future direction unless the owner explicitly
adds it to this release. For the current owner gate, distinguish safe key-backup
restoration from total key loss/compromise and decide whether D-16 remains the
interim policy. Do not promise existing identities can later gain guardians
without an authorized enrollment or migration procedure.

Research inspected docs and source; it did not execute contract tests, audit
contracts, verify deployed wallet versions, or implement any MCT recovery path.
Temporary downloads: `/tmp/mct-guardian-research/`.
