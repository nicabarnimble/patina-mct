---------------------------- MODULE PeerSplitReads ----------------------------
\* Inbound hello and call from one governed peer P, as the separate reads the
\* code makes. Source at c9f1060 (functions, not line numbers):
\*   HelloBindingRead   mct-iroh serve.rs, mct/hello/0 arm: bindings_provider()
\*                      -> daemon serving.rs read_peer_bindings_for_iroh:
\*                      ResidentLedgerWriter::governed_peers, then config.json
\*                      kept by retain_peers_named_by_ledger
\*   HelloReceiverRead  same arm: receiver_authority_provider.current()
\*                      -> ResidentLedgerWriter::projected_authority
\*   HelloRecheckRead   same arm: second bindings_provider() after the
\*                      receiver read; refuses when no longer admitted
\*   CallBindingRead    mct-iroh serve.rs, mct/call/0 arm: bindings_provider()
\*   CallReceiverRead   same arm: receiver_authority_provider.current(), then
\*                      mct-kernel call/internal.rs evaluate_call_protocol:
\*                      expected receiver identity == current (KernelCheck)
\*   DispatchCheck      daemon pipeline.rs execute_resident_call:
\*                      fresh_resident_receiver_identity, then
\*                      expected == current (PipelineCheck). Execution follows;
\*                      a revoke after this check applies to the next call.
\*   Revoke, OtherCommit governed passkey commits; each makes a new receiver
\*                      identity (observation passkey_admin.rs
\*                      PasskeyAdminFactV1::identity: generation + 1 and the
\*                      committing fact's hash)
\*   WriterFails        the resident writer fences or stops; every read through
\*                      it fails (governed_peers, projected_authority)
\* Assumptions:
\* - A caller can present only an identity an admitted hello returned. The
\*   identity names the committing fact's hash, which a caller cannot predict.
\* - Observation-sink writes are not modeled. In code a stopped or fenced
\*   writer also fails the hello's evaluation record, which ends the
\*   connection; the config-fallback counterpart shows the property without
\*   that second barrier.
\* - One peer, one hello and one call in flight at a time; bounds in the cfg.
EXTENDS Naturals, FiniteSets

CONSTANTS HelloRecheck, KernelCheck, PipelineCheck, ConfigFallback,
          MaxCommits, MaxHellos, MaxCalls

VARIABLES
    ledger, gen, revokedAt, commits, writerUp,
    h, hBind, hGen, hellos,
    known,
    c, cExp, cBind, cRecv, calls,
    dishonestHello, dispatchedWhileRevoked

vars == <<ledger, gen, revokedAt, commits, writerUp, h, hBind, hGen, hellos,
          known, c, cExp, cBind, cRecv, calls, dishonestHello, dispatchedWhileRevoked>>

\* P is admitted under generation g.
AdmittedAt(g) == revokedAt = 0 \/ g < revokedAt

\* What a binding read returns: the ledger through the writer, or an error.
\* ConfigFallback models reading a stale config copy that still says admitted.
BindingRead == IF writerUp THEN ledger ELSE IF ConfigFallback THEN "admitted" ELSE "error"

Init ==
    /\ ledger = "admitted" /\ gen = 1 /\ revokedAt = 0 /\ commits = 0 /\ writerUp = TRUE
    /\ h = "idle" /\ hBind = "none" /\ hGen = 0 /\ hellos = 0
    /\ known = {}
    /\ c = "idle" /\ cExp = 0 /\ cBind = "none" /\ cRecv = 0 /\ calls = 0
    /\ dishonestHello = FALSE /\ dispatchedWhileRevoked = FALSE

Revoke ==
    /\ commits < MaxCommits /\ writerUp /\ ledger = "admitted"
    /\ ledger' = "revoked" /\ gen' = gen + 1 /\ revokedAt' = gen + 1 /\ commits' = commits + 1
    /\ UNCHANGED <<writerUp, h, hBind, hGen, hellos, known, c, cExp, cBind, cRecv, calls, dishonestHello, dispatchedWhileRevoked>>

OtherCommit ==
    /\ commits < MaxCommits /\ writerUp
    /\ gen' = gen + 1 /\ commits' = commits + 1
    /\ UNCHANGED <<ledger, revokedAt, writerUp, h, hBind, hGen, hellos, known, c, cExp, cBind, cRecv, calls, dishonestHello, dispatchedWhileRevoked>>

WriterFails ==
    /\ writerUp /\ writerUp' = FALSE
    /\ UNCHANGED <<ledger, gen, revokedAt, commits, h, hBind, hGen, hellos, known, c, cExp, cBind, cRecv, calls, dishonestHello, dispatchedWhileRevoked>>

\* ---------------- hello
HelloBindingRead ==
    /\ h = "idle" /\ hellos < MaxHellos
    /\ hBind' = BindingRead /\ hellos' = hellos + 1
    /\ h' = IF BindingRead = "admitted" THEN "bound" ELSE "idle"
    /\ UNCHANGED <<ledger, gen, revokedAt, commits, writerUp, hGen, known, c, cExp, cBind, cRecv, calls, dishonestHello, dispatchedWhileRevoked>>

\* Admit with the identity just read, unless a recheck follows.
HelloReceiverRead ==
    /\ h = "bound"
    /\ IF ~writerUp
       THEN h' = "idle" /\ UNCHANGED <<hGen, known, dishonestHello>>
       ELSE IF HelloRecheck
            THEN h' = "received" /\ hGen' = gen /\ UNCHANGED <<known, dishonestHello>>
            ELSE /\ h' = "idle" /\ hGen' = gen /\ known' = known \union {gen}
                 /\ dishonestHello' = (dishonestHello \/ ~AdmittedAt(gen))
    /\ UNCHANGED <<ledger, gen, revokedAt, commits, writerUp, hBind, hellos, c, cExp, cBind, cRecv, calls, dispatchedWhileRevoked>>

HelloRecheckRead ==
    /\ h = "received" /\ h' = "idle"
    /\ IF BindingRead = "admitted"
       THEN /\ known' = known \union {hGen}
            /\ dishonestHello' = (dishonestHello \/ ~AdmittedAt(hGen))
       ELSE UNCHANGED <<known, dishonestHello>>
    /\ UNCHANGED <<ledger, gen, revokedAt, commits, writerUp, hBind, hGen, hellos, c, cExp, cBind, cRecv, calls, dispatchedWhileRevoked>>

\* ---------------- call
CallBindingRead ==
    /\ c = "idle" /\ calls < MaxCalls
    /\ \E e \in known :
        /\ cExp' = e /\ calls' = calls + 1 /\ cBind' = BindingRead
        /\ c' = IF BindingRead = "admitted" THEN "bound" ELSE "idle"
    /\ UNCHANGED <<ledger, gen, revokedAt, commits, writerUp, h, hBind, hGen, hellos, known, cRecv, dishonestHello, dispatchedWhileRevoked>>

CallReceiverRead ==
    /\ c = "bound"
    /\ IF ~writerUp \/ (KernelCheck /\ cExp # gen)
       THEN c' = "idle" /\ cRecv' = 0
       ELSE c' = "evaluated" /\ cRecv' = gen
    /\ UNCHANGED <<ledger, gen, revokedAt, commits, writerUp, h, hBind, hGen, hellos, known, cExp, cBind, calls, dishonestHello, dispatchedWhileRevoked>>

DispatchCheck ==
    /\ c = "evaluated" /\ c' = "idle"
    /\ IF ~writerUp \/ (PipelineCheck /\ cExp # gen)
       THEN UNCHANGED dispatchedWhileRevoked
       ELSE dispatchedWhileRevoked' = (dispatchedWhileRevoked \/ ledger = "revoked")
    /\ UNCHANGED <<ledger, gen, revokedAt, commits, writerUp, h, hBind, hGen, hellos, known, cExp, cBind, cRecv, calls, dishonestHello>>

\* Nothing in flight: the Mother waits. Stuttering here is not a deadlock.
Done ==
    /\ h = "idle" /\ c = "idle"
    /\ UNCHANGED vars

Next ==
    \/ Revoke \/ OtherCommit \/ WriterFails
    \/ HelloBindingRead \/ HelloReceiverRead \/ HelloRecheckRead
    \/ CallBindingRead \/ CallReceiverRead \/ DispatchCheck
    \/ Done

Spec == Init /\ [][Next]_vars

\* An admitted hello names an identity under which P was admitted.
HelloNamesAdmittedIdentity == ~dishonestHello
\* A call passes its dispatch check only while P is admitted in the ledger.
RevokedPeerNeverDispatched == ~dispatchedWhileRevoked
================================================================================
