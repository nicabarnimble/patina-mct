# Pi Durable as a Child

> **Imported 2026-10-18** from working notes (2026-10-14). Status: design note. Three conflicts are ruled closed inside it; four stay **open** (listed in [../OPEN-QUESTIONS.md](../OPEN-QUESTIONS.md)). Nothing in it is built. File and line citations into `earendil-works/pi` and `patina-wasi-pi` are to the commits named in the Sources section, not to this repo.


A design note. It places Pi Durable inside MCT as a Child. (a)–(f) in [admin-keys.md](admin-keys.md) stay locked. The rulings already in that note stay as written: Mother mints every borrowed handle, resuming from a freeze counts as a wake under (e), state goes to the model only after check-in, "do this next" is the interrupt carrying the next task, a fix is a grant, Mother never reaches into Child state, extend with a Toy and never fork the standard, the replay Toy sits beside `wasi:random`, and the gateway only forwards while Mother's grants enforce.

This note does not rewrite those decisions. Three fights are closed below by rulings in this note: per-tool replay, durable-object self-wake, and host-owned session storage. The fights this message does not rule stay open in [Contradictions to discuss](#contradictions-to-discuss).

## Sources

Read for this note:

- [admin-keys.md](admin-keys.md), the decided list and the rulings above.
- `earendil-works/pi` `packages/durable` at `b45e0ebcc5ae2fc42a6f2f23104d10e656e6bedb` (`main` when fetched): `README.md`; `docs/spec.md`; `src/harness/harness.ts`, `generation.ts`, `scheduler.ts`, `tool.ts`, `types.ts`, `provider.ts`; `src/codemode/codemode.ts`, `models.ts`; `src/tools/bash.ts`, `read.ts`; `src/env/node.ts`; `src/storage/sqlite/cloudflare.ts`.
- `packages/ai/src/providers/openai.ts` at the same commit. The body of `envApiKeyAuth` was not read.
- Cloudflare's Pi harness page, [Pi · Cloudflare Agents](https://developers.cloudflare.com/agents/harnesses/pi/), as fetched for this note (page date 12 Oct 2026). The Agents SDK source was not read, so this note cites no line numbers from `PiHarness`.
- patina-mct `crates/mct-daemon/src/wasm.rs` as in this checkout: `build_wasi_ctx`, `is_supported_wasi_p2_import`, `link_wit_host_import_adapters`.

Reported, not read. A read-only audit of `patina-wasi-pi` `main` at `10184f3` supplied the line numbers below, and the same audit and its docs supplied the limitation figures. The docs named for those figures are `docs/SNAPSHOT.md`, `bench-snapshot-output.md`, and `DEFAULTS.md`. This note did not open that repo and invents no other lines or numbers from it.

Not read, so not verified: `@earendil-works/pi-codemode` (the QuickJS sandbox that receives the seed and clock), `jco`, StarlingMonkey, and the rest of the Pi monorepo. A search of the durable files listed above found no `jco`, no `StarlingMonkey`, and no `wasi:` import.

## What stays locked

From the decided list:

- **(a)** Admin keys. The admin side makes the keypair. Mother stores the public half. Grants are scoped, ledgered, and revocable, except the recovery grant.
- **(b)** Iroh is the only admin transport.
- **(c)** A Child refuses to act without Mother. Mother unreachable is a hard stop for a Child's work, not failover. The effort goes into Mother never dying.
- **(d)** A Child is a durable object. The only way in is a platform-given handle, and holding the handle is the authorization.
- **(e)** On wake, a Child checks in with Mother before it touches its own state. The handle proves identity. The check-in proves it is still her Child and that policy is current. Resuming from a freeze counts as a wake.
- **(f)** A Child continues its current task until it needs another one. Mother interrupts by an iroh message, checked at step checkpoints. If Mother drops, the Child finishes the current step and freezes at the next checkpoint.

Rulings this note carries whole:

- Mother mints every borrowed handle. The lending Child asks. A Child minting a handle stays closed.
- Resuming from a freeze reuses the wake check-in. If the check-in fails, the Child stays frozen.
- State goes to the model only after that check-in.
- "Do this next" is the interrupt carrying the next task. The Child still acts only at its checkpoints.
- A fix is a grant: a fix Toy, a corrected input, or a restart with new parameters. Mother does not rewrite the Child's state.
- Extend with a Toy, never fork the standard. The replay Toy sits beside `wasi:random`. Secure random stays real entropy. A replayed run reuses the recorded model answer instead of calling the model again.
- The gateway holds the credential, honors the handle Mother already granted, and forwards. It does not decide who may call. Mother's grant check is the enforcement.

Rulings added in this note. They do not rewrite locked (a)–(f).

- Per-tool `replay: "safe"` collapses into the replay Toy: one replay path. Cheap reads reuse recorded results through the ledger's hash-check pattern. There are no per-tool flags.
- Durable-object self-wake is not an exception. The Child requests a wake through a Toy and Mother fires it. The Child checks in before any model call.
- Host-owned session storage adapts to ledger-backed, grant-gated state. Mother never reaches into Child state. "Do this next" and fixes-as-grants still hold.

## Pi Durable running as a Child

Pi Durable is the harness: one storage, conversations as transcripts, tasks that checkpoint each step, and a host that calls `Harness.open`, `resume`, `submit`, and `abort` (`packages/durable/README.md`; `harness.ts` `Harness.open` at 446–469, `resume` at 242–245). In MCT that harness is a Child. It is one durable object under (d). It is not a Node process beside Mother, and it is not a Cloudflare Worker principal that Mother talks to as a peer. Pi is never an external native worker principal.

The Child can do nothing on its own. Files, processes, clocks beyond the standard import, model calls, and any handle it lends all go through Toys Mother granted. Holding the platform handle is what lets Mother reach the Child. It is not a license for the Child to open a provider, a directory, or a sibling's Toy. `NodeExecutionEnv` (`env/node.ts`) and `openaiProvider()` (`packages/ai/src/providers/openai.ts`) are host powers in the package as it stands. Inside MCT those powers are Toys, or they are absent.

What the Child does with a task it already has is (f): it continues that task inside the Toys it was given, and it listens at checkpoints. A steer in Pi is placed after the current tool round (`README.md`, Busy Conversations). That is the same pause (f) already names. The message at that pause is "do this next" when Mother puts the next task in the interrupt.

## Clock Toy

The experiment, as reported from `patina-wasi-pi` `10184f3`: virtual time is anchored to recorded Toy calls (`clocks.rs:72-76`); without a grant the clock is fixed time (`clocks.rs:11-25`); resume reuses a recorded `t0_ms` (`main.rs:423`); the clock is its own interface in WIT (`patina-pi.wit:108`). The WASI clock path there is a host override through `VClock`.

MCT does not copy that override. `wasi:clocks/monotonic-clock@0.2.3` and `wasi:clocks/wall-clock@0.2.3` are ordinary supported imports (`wasm.rs` `is_supported_wasi_p2_import`, 1613–1614), linked with the rest of `wasi:p2` by `wasmtime_wasi::p2::add_to_linker_sync` (`link_wit_host_import_adapters`, 1627–1633). `build_wasi_ctx` (343–397) configures preopens only. It installs no clock. This note did not read wasmtime's clock host, so it does not describe that default beyond "the standard linker, with no MCT override."

Time the Child wants to record and resume is a Clock Toy, a separate WIT interface, same rule as the replay Toy beside `wasi:random`. The Toy is granted or it is not there. When it is granted, its time is the virtual clock anchored to recorded Toy calls, and a resume reuses the recorded `t0` the Toy already stored. `wasi:clocks` stays the standard import. The Child that needs deterministic time asks for the Clock Toy. It does not get that time by a host object standing in for `wasi:clocks`.

Pi's own clock is a host function. `HarnessOptions.now` defaults to `Date.now` (spec.md:372). `safeNow` (`harness.ts:388–401`) returns `Date.now` when the host function is absent, and again when that function throws. Code mode draws `Date.now()` and a `crypto.getRandomValues` seed, stores them on the run document only when the run has no start yet (`codemode.ts:398–421`), and passes that stored pair back into the sandbox on resume (`codemode.ts:460–462`). The deadline check on resume uses `Date.now()` again (`codemode.ts:451`). How the sandbox applies the seed and the virtual clock lives in `@earendil-works/pi-codemode`, which this note did not read.

## Credentials stay with Mother

The experiment, as reported: Mother injects credentials on the host side only after the gate approves (`toys.rs:157-158`); the xAI bearer never leaves the gateway (`xai_gateway.rs:586-600`); the guest gets no preopens (`main.rs:296-298`).

That is the MCT shape. Mother holds the credential. She grants a Toy. The gateway forwards the call and does not decide who may call. The Child holds the Toy handle and never the bearer. A preopen, a process environment, or a provider object inside the guest is the credential leaving Mother.

What Pi does today is the opposite arrangement, and it is named as a contradiction below. The quick start calls `openaiProvider()`, commented as reading `OPENAI_API_KEY` (`README.md:57`). The provider registers `auth.apiKey: envApiKeyAuth("OpenAI API key", ["OPENAI_API_KEY"])` (`openai.ts:9–15`). Generation then calls `runtime.models.streamSimple` with the resolved model (`generation.ts:431`). The tool API exposes that same `models` object as "the catalog, credentials, and request transforms generation uses" (`types.ts:213`). Code mode drops `headers` from the catalog a script can see, because headers can carry credentials (`models.ts:87–91`), and it resolves a script's model by provider and id so a script-supplied `baseUrl` or headers never receive the credentials (`models.ts:197–199`). The credentials remain on the `Models` object the harness calls. `bash` defaults `inheritEnv: true` (`bash.ts:72–76`). `getShellEnv` copies `process.env` when that flag is set (`env/node.ts:261–270`).

`build_wasi_ctx` still only preopens directories the call's Toy authority admits (`wasm.rs:348–396`). No preopen is the default. A filesystem Toy is a granted preopen, not an open root.

## Delegation, and the chain that is not there yet

The experiment, as reported: grant and revoke are ledgered (`core.rs:879`, `core.rs:886`), and chain registration is its own entry (`chains.rs:60-75`). The gap: grants carry no id and no parent link, and revoke flips every `(toy, op)` match (`grants.rs:53-57`). There is no minted-handle-from-grant-to-borrower chain yet.

That gap is the courier ruling waiting on a ledger shape. Mother is the only grantor. She mints every borrowed handle. The older Child asks to share a handle it already holds, or a smaller one, never a bigger one. The ledger row has to name the parent grant and the new handle, so a revoke of the parent takes every borrowed copy, and a revoke does not also flip an unrelated grant that happens to name the same toy and operation. A revoke that matches every `(toy, op)` cannot express that chain. This note does not invent the fact schema. It records that the experiment's grant row cannot be that chain, and that a Child minting the borrowed handle stays the closed Child-as-grantor conflict.

Pi's subagent tool is a different mint, and it is a contradiction below. Inside the package, a task-owned conversation starts as a copy of its owner's agent: model, extensions, tools, and `cwd` (`README.md`, Per-Conversation Agent and the subagent example at 539–567). The tool creates that conversation in its own commit. Mother is not in that path.

## Wake check-in

Decision (e) is locked. The experiment has not built it. As reported from `10184f3`: wakes are stored and listed (`core.rs:23`, `core.rs:46-49`; `demo_persist.rs:187`). Nothing fires a wake. Nothing makes the Child check in before a model call. The gateway checks the grant (`toys.rs:156`) and then injects the credential. A grant check is not a wake check-in. It does not prove the Child is still hers, and it does not prove the policy generation.

Pi Durable's loop is built around resuming, so this is the piece it actually needs. `Harness.open` calls `openTasks` before it returns (`harness.ts:459–461`). `open` loads live tasks and commits every surviving `running` task back to `pending`, keeping the checkpoint (`scheduler.ts:288–305`). That commit is the Child's storage, written on the way back in. `resume` then enables the scheduler (`scheduler.ts:317–320`; `harness.ts:242–245`). The `request` phase of `pi.generation` builds the transcript and calls `streamSimple` (`generation.ts:196–221`, `431`). There is no check-in in that path.

The self-wake clash is closed. Durable-object self-wake is not an exception. The Child requests a wake through a Toy and Mother fires it. The Child checks in before any model call. That ties to (e) and to freeze-as-wake: resuming from a freeze is a wake, Mother's answer is the go-ahead and the policy re-check, and a failed check-in leaves the Child frozen. (e) still says the check-in happens before the Child touches its own state. The new sentence does not move that check-in to after the state write, and it does not narrow (e) to the model call alone. State still goes to the model only after the check-in.

Pi's `open` write and its `streamSimple` call are what that ruling replaces. The Child does not arm its own alarm, and a Cloudflare Lifecycle alarm is not a second door under (d). If Mother is unreachable she does not fire the wake, which is the hard stop in (c), not a Child that continues. (a), (b), and (f) are not rewritten. (f) still freezes the Child at the next checkpoint when Mother drops. The wake Toy is how she brings it back, after the check-in.

## Rules that stay in force

**Extend with a Toy, never fork the standard.** The cautionary tale is the experiment's `wasi:random` fork, as reported: `main.rs:286-290` replaces secure and insecure random with a seeded stream; the seed is written in plain text into the run entry (`main.rs:430-431`) and into snapshots (`main.rs:459`), so the secure output is predictable. The fix already ruled is the replay Toy beside untouched `wasi:random`. Secure `get-random-bytes` stays real entropy. The Child asks for the replay Toy when it wants a seed and a counter. The same rule is why the Clock Toy sits beside `wasi:clocks` and does not replace it, and why wake is its own Toy. The admin-keys note already closed priority 1 that way: wake and replay stay different Toys, and that note does not decide wake.

Per-tool `replay: "safe"` collapses into that same replay Toy. There is one replay path, and there are no per-tool flags. A replayed run reuses the recorded model answer. Cheap reads reuse recorded results through the ledger's hash-check pattern, reported from `docs/SNAPSHOT.md` at `10184f3`: recording levels are full, hash, and presence. Hash re-runs cheap reads and compares them to the sha. This note does not describe what full or presence store beyond those names, and it does not choose what a sha mismatch does.

**"Do this next" is the interrupt carrying the next task.** Pi already has a boundary after the current tool round where a steer joins the running work. In MCT that boundary is the checkpoint in (f), and the payload is the next task. It is one message. It is not a second channel that edits the transcript from outside.

**Fixes come only through grants.** A fix Toy, a corrected input, or a restart with new parameters. Taking a Toy back is revocation under (a). Mother never reaches into the Child's documents to correct them.

**Session state is ledger-backed and grant-gated.** Host-owned session storage adapts to that. Anything Pi would write to its own session is a Toy call Mother granted, with the result recorded in the ledger. It is not an in-place write by whoever holds the harness, and it is not Mother reaching into the Child's state. "Do this next" stays the interrupt carrying the next task. A fix stays a grant.

**The gateway only forwards.** Mother's grant is what allows the model call. The gateway does not apply older-child status, the per-task rule, or the wake check-in. Those are Mother's checks, made before the handle exists.

## Limitations

The ledger-backed, grant-gated model imposes four limits on Pi Durable. They are not locked (a)–(f), and they do not rewrite them. Figures and file names here are reported from the wasi-pi audit and its docs at `main` @ `10184f3`: `docs/SNAPSHOT.md`, `bench-snapshot-output.md`, and `DEFAULTS.md`. This note did not open that repo.

### Resume cost is a full re-run

The snapshot spike showed Wasmtime can't snapshot a running Child, so a resumed Child re-executes from the top. Open plus resume stayed around 136–165 ms either way. The savings are storage, not speed. Long-run bytes fell from 1,129,679 to 479,083, or 321,827 with read-file stored as a hash (`docs/SNAPSHOT.md`, `bench-snapshot-output.md`).

That re-execution is the one replay path. Guest code runs from the top. Recorded Toy results come back from the ledger, including the recorded model answer, and cheap reads take the hash level: re-run and compare to the sha. The benchmark does not say the re-execution may start before the wake check-in. (e) still requires the check-in before the Child touches its own state, and the wake ruling still requires it before any model call. (f) is not rewritten: the Child is still on its current task. The milliseconds say that task does not get faster by being durable.

### Session state can't be mutated in place

Session state goes through the ledger. Anything Pi would write to its own session becomes a grant-gated Toy call with a recorded result. `configure`, `reset`, and the `open` commit that moves `running` tasks back to `pending` are that kind of call once the harness is a Child, or they are not available. Mother still does not reach in. The ledger line is the Toy call's recorded result, not a rewrite of the Child's state by Mother. "Do this next" and fixes-as-grants still hold.

(e) still comes first. A session Toy call is the Child touching its own state, so it waits for the check-in. This ruling does not allow the `open` write to land before that check-in.

### Fingerprint rule

Resume refuses if the code hash changes (`DEFAULTS.md`, the run credential). Upgrading the Pi harness breaks runs already in flight.

No conflict with (a)–(f) is decided by that refusal. (f) still says the Child continues its current task, and freezes at the next checkpoint if Mother drops. (e) still says a freeze-resume is a wake and a check-in. The fingerprint rule adds a refusal when the hash changes. That refusal is not Mother's interrupt under (f), and it is not failover under (c). What the Child does after the refusal, stay frozen or have no run to resume, is not ruled here.

### Pi Durable becomes a durable agent, not a fast one

Pi Durable becomes a durable agent, not a fast one. The ledger is the system of record. The 136–165 ms open-plus-resume, either way, is the speed this model has. Durability shows up as fewer stored bytes, not as a shorter resume.

## Contradictions to discuss

Three are closed by the rulings above. Four stay open. Closed means the Pi shape adapts. It does not mean the experiment has built the check-in, and it does not rewrite locked (a)–(f).

### Per-tool replay: "safe" against the replay Toy and recorded model answers

Closed. Per-tool `replay: "safe"` collapses into the replay Toy: one replay path. Cheap reads reuse recorded results through the ledger's hash-check pattern. There are no per-tool flags.

What Pi does today stays the evidence that had to move. `ToolRegistration.replay` is `"safe" | "unsafe"`, and omitted means `unsafe` (`types.ts:296–298`; spec.md:2896, and the sentence at spec.md:2927). On recovery the `execute` phase reruns only when the stored intent and the current declaration both say `safe` (`tool.ts:168–180`; spec.md:3376–3381). Otherwise the model gets an interrupted result. `read`, `write`, `edit`, and `bash` do not set `replay` in the files read (`bash.ts`, `read.ts`), so they take the default. Code mode sets `replay: "safe"` (`codemode.ts:240`) and, on rerun, feeds the sandbox the stored `seed` and `startedAt` plus a journal of results (`codemode.ts:10–15`, `398–421`, `460–462`). When the model resolves, the `request` phase calls `streamSimple` again (`generation.ts:205–206`, `431`). The Cloudflare page says the same for a crash mid-stream: Pi keeps the partial it stored and makes the model call again.

That split is not kept. Guest code still re-executes from the top, because Wasmtime can't snapshot a running Child. The model answer is a recorded result, not a second provider call. Cheap reads use the hash level from `docs/SNAPSHOT.md`: re-run and compare to the sha. Recording levels named there are full, hash, and presence. A code-mode seed stored on the run document is not a second replay path and is not a per-tool flag. This note does not choose the sha-mismatch outcome, and it does not describe full or presence beyond the names.

No locked letter (a)–(f) is rewritten. The replay Toy still sits beside `wasi:random`. A replayed run still reuses the recorded model answer.

### Durable-object self-wake against Mother-fired wakes and (c)

Closed. Durable-object self-wake is not an exception. The Child requests a wake through a Toy and Mother fires it. The Child checks in before any model call.

What the package and the Cloudflare page do stays the evidence. The package does not set an alarm. `cloudflare.ts` adapts `ctx.storage.sql` (`openDurableObjectSqliteStorage`, 136–138). The scheduler's wakeup is in-process (`scheduler.ts:1094–1114`). `resume` is a host call (`harness.ts:242–245`). The docs page says `PiHarness` schedules a Lifecycle job, the alarm restarts an evicted object, and the run continues, with a heartbeat every 30 seconds. Nothing on that page asks a Mother. The Durable Object name (`getByName` on that page) admits the restart.

That continue-without-Mother is not kept. The Child's request is a Toy call. Mother fires the wake. The check-in is the one (e) already names, including when the resume is a freeze-as-wake, and it happens before any model call and before the Child touches its own state. (c) stays a hard stop: if she is unreachable she does not fire it, and the Child does not keep working. (d) stays the platform handle. The alarm is not a second door, and it is not an admin transport, so (b) is untouched. (a) is untouched. (f) still freezes the Child at the next checkpoint when Mother drops. The wake Toy is how a frozen Child asks to come back. It is not permission to skip the freeze.

### Host-owned session storage against Mother never reaching into Child state

Closed. Host-owned session storage adapts to ledger-backed, grant-gated state. Mother never reaches into Child state. "Do this next" and fixes-as-grants still hold.

What Pi does today stays the evidence. `Harness.open` takes the storage the host hands it (`harness.ts:446–451`). One process owns that storage (`README.md`, Storage). The host then has `configure` (`harness.ts:99–101`), `reset` (`harness.ts:113–119`), `abort` (`harness.ts:149–151`), `submit`, and `viewState` (`harness.ts:159–161`). `configure` writes `pi.agent`. `reset` appends a reset entry. `open` commits `running` tasks back to `pending` (`scheduler.ts:303–304`). On Cloudflare the object's SQLite database is shared: the docs page says Pi's tables are prefixed `pi_` so they do not collide with the host's own tables.

Those host writes are not how a Child keeps state. Session state can't be mutated in place. It goes through the ledger as a grant-gated Toy call with a recorded result. Mother does not open that storage and correct it. A next task arrives as the interrupt. A fix arrives as a grant. (e) still requires the check-in before those Toy calls, because they are the Child touching its own state. (a) and (f) stay as written. (b), (c), and (d) are not what this clash was about.

### Ambient credentials and direct model calls against the gateway

Still open. `openaiProvider` registers an API key taken from `OPENAI_API_KEY` (`openai.ts:15`; `README.md:57`). This note did not read `envApiKeyAuth`'s body. Generation calls `streamSimple` on the harness's `Models` (`generation.ts:205–219`, `431`). The tool API's `models` field is documented as carrying credentials (`types.ts:213`). Code mode keeps those credentials off the script-visible catalog (`models.ts:87–91`, `197–199`) and still calls through the harness. `bash` inherits `process.env` by default (`bash.ts:76`, `env/node.ts:261–270`), so a key sitting in the process environment is visible to a shell the model asked for.

The ruling is that Mother holds the credential, the Child holds the Toy, and the gateway forwards. The experiment's reported shape matches that ruling: inject after the gate, bearer stays in the gateway, no preopens. A harness that owns `Models` and calls the provider is the Child calling with the credential in hand. That is the closed direct-hosted-credential flag in the admin-keys note. `inheritEnv` is a second leak of the same secret, through the process environment rather than through the provider object.

This note does not wrap `streamSimple` in the gateway and call the contradiction closed. The wake ruling already requires a check-in before any model call. That does not decide who holds the credential. (a) is untouched in the sense the admin-keys note already gave: this credential is not an admin key and is not the recovery grant. (d) is involved, because the model Toy is a handle Mother grants, not a provider object the harness constructed. (b) and (f) are not what this clash is about. A model call while Mother is unreachable is the hard stop in (c), which the wake ruling already keeps.

### A JS Child via jco or StarlingMonkey against WASI standards

Still open. The durable package at `b45e0eb` is TypeScript. It runs on Node (`NodeExecutionEnv`, `process.env`, `spawn` in `env/node.ts`) and inside a Cloudflare Durable Object with `nodejs_compat` (the docs page's Wrangler snippet). The files searched do not mention `jco`, StarlingMonkey, or a WASI import. This note did not read `jco` or StarlingMonkey, and it did not find a component build of this package.

MCT's Child is a WIT/WASI component. The linker path above is standard `wasi:p2` plus granted host imports. "Extend with a Toy, never fork the standard" means a JS runtime does not become a private set of host builtins for the clock, for random, for the filesystem, or for the model HTTP call.

A native Node worker beside Mother is the external principal this note already rules out. A component build that reaches those powers through engine builtins, instead of through `wasi:clocks`, `wasi:random`, `wasi:filesystem`, `wasi:http`, and MCT Toys, would be the same kind of fork as the `VClock` override and the seeded `wasi:random`. This note cannot say whether `jco` or StarlingMonkey do that, because those trees were not read. It also cannot say a component build is clean. The question stays open. (d) is the letter in play: the way in has to be the platform handle on a component, not a JS host with ambient Node powers. (b) is untouched.

### Subagent copies of the parent's tools against Mother minting every borrowed handle

Still open. The subagent tool, marked `replay: "safe"` in the package, creates or reuses a conversation owned by the tool task and starts it as a copy of the caller's agent (`README.md:539–567`). The spec's sketch then `configure`s the child to drop the subagent extension (`spec.md:3354–3358`). The copy still carries the parent's model, tools, and `cwd` unless something narrows them. The tool does this in `api.commit`. Mother does not mint a handle.

The courier ruling says Mother creates every borrowed handle, the share is an attenuation of a handle the older Child already holds, and a young Child does not lend. A conversation that starts with the parent's tool list is a copy of power, made by the Child, with no parent-grant id and no minted handle. That is the closed Child-as-grantor conflict, and it is the same hole as the experiment's grants that revoke by `(toy, op)` with no parent link: there is nothing to follow when the parent's Toy is taken back.

This note does not turn the subagent conversation into a Mother-minted handle, and it does not declare subagents out of scope. The `replay: "safe"` mark on that tool is not a second replay path. That flag is closed above. The copy of the parent's tools is the fight that stays open. (a) and (d) are the letters in play. (b), (c), (e), and (f) are untouched by this clash.

### Background work that survives abort against a stop

Still open. Pi's `background: true` task is a boundary: work it owns survives the parent's abort and does not keep the parent busy (`README.md`, Abort and Subagents). `root.abort(context, { background: true })` is what reaches it. A stop that calls `abort` without that flag leaves the background work running.

The adopted narrative says Mother can stop a Child. (f) says the interrupt is checked at a checkpoint, and the Child finishes the current step. This note does not decide that Mother's stop is `Conversation.abort`. If it is, background work outlives the stop. If it is not, this flag is only a mapping hazard. Left unresolved on purpose. (c) is nearby: background work that keeps going while Mother is unreachable is the Child acting without her.
