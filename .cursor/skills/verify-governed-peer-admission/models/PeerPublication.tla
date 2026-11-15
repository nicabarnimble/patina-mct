---------------------------- MODULE PeerPublication ----------------------------
\* NIC-131: one governed Mother, one peer P, its ledger, its config.json copy,
\* concurrent publications of that copy, and inbound admission of P.
\* Repaired = TRUE describes commit 4c22919 and later; FALSE describes 503d0e9.
\* Admit is one atomic step here. It checks only which source admission reads
\* (ledger or copy). The split reads of hello and call are modeled separately
\* in PeerSplitReads.tla; this module makes no claim about them.
\*   serving.rs load_peer_bindings_for_iroh       -> Admit reads ledger or copy
\*   serving.rs project_governed_config_from      -> Lock, Capture, Write, Fail
\*   serving.rs admin handler (Completed|Indet.)  -> which replies start a publication
\*   passkey_admin.rs commit then post-commit check -> reply Completed or Indeterminate
EXTENDS Naturals, FiniteSets

CONSTANTS Repaired, MaxCommands, Pubs

VARIABLES
    ledger,     \* P in the ledger: "admitted" or "revoked"
    gen,        \* ledger generation; every committed command advances it
    commands,   \* committed commands so far (bound)
    copy,       \* P in config.json
    copyGen,    \* generation config.json was written from
    pub,        \* per publication: phase and captured snapshot
    lock,       \* publication holding the lock, or 0
    admittedWhileRevoked, \* an inbound call was admitted after the ledger revoked P
    regressed   \* a write replaced the copy with an older generation

vars == <<ledger, gen, commands, copy, copyGen, pub, lock, admittedWhileRevoked, regressed>>

Phases == {"idle", "waiting", "captured"}

Init ==
    /\ ledger = "admitted" /\ gen = 1 /\ commands = 0
    /\ copy = "admitted" /\ copyGen = 1
    /\ pub = [p \in Pubs |-> [phase |-> "idle", state |-> "admitted", g |-> 0]]
    /\ lock = 0
    /\ admittedWhileRevoked = FALSE /\ regressed = FALSE

\* A committed command starts publication p when its reply allows it.
Start(p, reply) ==
    IF reply = "completed" \/ (reply = "indeterminate" /\ Repaired)
    THEN pub' = [pub EXCEPT ![p].phase = "waiting"]
    ELSE pub' = pub

RevokeCommits(p, reply) ==
    /\ commands < MaxCommands /\ ledger = "admitted" /\ pub[p].phase = "idle"
    /\ ledger' = "revoked" /\ gen' = gen + 1 /\ commands' = commands + 1
    /\ Start(p, reply)
    /\ UNCHANGED <<copy, copyGen, lock, admittedWhileRevoked, regressed>>

OtherCommandCommits(p) ==
    /\ commands < MaxCommands /\ pub[p].phase = "idle"
    /\ gen' = gen + 1 /\ commands' = commands + 1
    /\ Start(p, "completed")
    /\ UNCHANGED <<ledger, copy, copyGen, lock, admittedWhileRevoked, regressed>>

\* Repaired: take the lock first, then read the head. Unrepaired: no lock.
Capture(p) ==
    /\ pub[p].phase = "waiting"
    /\ Repaired => lock = 0
    /\ lock' = IF Repaired THEN p ELSE lock
    /\ pub' = [pub EXCEPT ![p] = [phase |-> "captured", state |-> ledger, g |-> gen]]
    /\ UNCHANGED <<ledger, gen, commands, copy, copyGen, admittedWhileRevoked, regressed>>

Write(p) ==
    /\ pub[p].phase = "captured"
    /\ copy' = pub[p].state /\ copyGen' = pub[p].g
    /\ regressed' = (regressed \/ pub[p].g < copyGen)
    /\ pub' = [pub EXCEPT ![p].phase = "idle"]
    /\ lock' = IF lock = p THEN 0 ELSE lock
    /\ UNCHANGED <<ledger, gen, commands, admittedWhileRevoked>>

\* The write fails; reads of the old file still work.
Fail(p) ==
    /\ pub[p].phase = "captured"
    /\ pub' = [pub EXCEPT ![p].phase = "idle"]
    /\ lock' = IF lock = p THEN 0 ELSE lock
    /\ UNCHANGED <<ledger, gen, commands, copy, copyGen, admittedWhileRevoked, regressed>>

\* A fresh hello and call from P. Repaired: bindings come from the ledger.
Admit ==
    /\ (IF Repaired THEN ledger ELSE copy) = "admitted"
    /\ admittedWhileRevoked' = (admittedWhileRevoked \/ ledger = "revoked")
    /\ UNCHANGED <<ledger, gen, commands, copy, copyGen, pub, lock, regressed>>

Done ==
    /\ commands = MaxCommands /\ \A p \in Pubs : pub[p].phase = "idle"
    /\ UNCHANGED vars

Next ==
    \/ \E p \in Pubs, r \in {"completed", "indeterminate"} : RevokeCommits(p, r)
    \/ \E p \in Pubs : OtherCommandCommits(p) \/ Capture(p) \/ Write(p) \/ Fail(p)
    \/ Admit
    \/ Done

Spec == Init /\ [][Next]_vars

\* Rules the reader agrees to.
RevokedPeerNeverAdmitted == ~admittedWhileRevoked
CopyNeverGoesBack == ~regressed
=====================================================================================
