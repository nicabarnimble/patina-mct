---
name: ub-mode
description: >-
  Follow Uncle Bob's reduced software engineering principles (First Make it
  Work, Then Make it Right with crapper/dryer/mutator, Follow Practice) and the
  user's "if this, then this (and why)" stack compass (Clojure data-oriented
  design, Rust backends in Jon Gjengset/Patina style with SQLite/rusqlite,
  Datastar + TypeScript/Effect for web UIs, SwiftUI for iOS, no Python unless
  needed, "Py"/"PyBot"/"Pie" in voice-to-text always means Pi and never Python,
  and zero hardcoded personal data in repos) whenever writing, refactoring, or
  reviewing code, or when the user invokes /ub-mode.
---
Every time you are asked to make a change follow:
* First Make it Work.
* Then Make it Right.
* Follow Practice

# Stack & Design Compass ("If This, Then This — And Why")

These are conditional defaults, not rigid dogma. Always start from **why** a layer or tool is being chosen for the problem at hand. When a task calls for a language or layer below, reach for the user's preferred way of building in it rather than improvising—and if the problem genuinely warrants a different tool, state why plainly.

* **Design Philosophy (Across Everything):**
  * Look to **Clojure** as a design practice—plain immutable data, pure domain functions at the core, explicit separation of state/identity/time, and clear atomic write boundaries (like Atlas's `POST /api/tx` with a `:reason` and revision check).
  * **Zero Hardcoded Personal State in Code:** Keep repositories general and installable by anyone. Never hardcode personal names, schools, dates, emails, or household-specific rules into application code or HTML templates; load household/instance config and data from external configuration and the local SQLite database (gitignored from the repo).
  * **Avoid Python Unless Strictly Needed (and Never Confuse Voice-to-Text "Py" with Python):**
    * Do not reach for Python for backends, build scripts, or CLIs when Rust or TypeScript/Effect/Node can do the job cleanly. Only use Python when a specific library ecosystem (such as an unavoidable 3D/CAD bridge) requires it, and state why.
    * **Voice-to-Text Homophone Rule:** When the user says or voice-dictates `"Py"`, `"PyBot"`, `"PiBot"`, `"Pie"`, `"Pie Bot"`, `"Py Grok"`, `"PyMux"`, or `"PyD"`, they **ALWAYS** mean **Pi** (`@Pi Grok`, `earendil-works/pi`, `pimux`, `pi-durable`, `PiD`), **NEVER Python**. Never assume `"Py"` is a request for a Python script or Python implementation.
* **If Backend / Server / Systems → Reach for Rust (`patina` style, Jon Gjengset North Star):**
  * **Type & API Design:** Follow Jon Gjengset (*Rust for Rustaceans*)—use the type system to make illegal states unrepresentable (newtypes, typestate, borrowed views over unnecessary allocations, `zerocopy` where layout matters, minimal dependencies, clear ownership boundaries).
  * **Concurrency:** Default to synchronous, threaded Rust first—`rayon` for data parallelism, OS threads, `parking_lot`, blocking `reqwest` (`rustls-tls`), and lightweight HTTP/SSE servers (as in `patina`)—rather than defaulting to heavy `tokio`/`async` stacks everywhere. Only bring in `async`/`tokio` when a specific boundary (like `iroh`) needs it, and explain why.
  * **Storage & Sandboxing:** Bundled `rusqlite` (SQLite WAL mode, single-writer), `duckdb` when analytical/vector queries call for it, and WASI/WASM components (`wasmtime` + WIT contracts) for sandboxed plugins/children.
* **If Persistence / Storage → Reach for SQLite first:** Prefer local-first **SQLite** (WAL mode, single-writer per database/household, stored outside the code repo) over cloud KV stores or heavy client-server DBs, unless deployment constraints specifically require something else.
* **If Browser / Web UI → Reach for Datastar + TypeScript/Effect 4.0:** Keep frontends clean and reactive with vendored **Datastar** (`v1.0.4+` server-rendered HTML fragments + SSE signals) and **TypeScript + Effect 4.0** (`Schema` validation at boundaries, typed `Context`/`Layer` services, explicit tagged errors) talking to the Rust (or Babashka/Atlas) backend.
* **If Apple / iOS / macOS Native UI → Reach for SwiftUI:** When building for iOS or Apple platforms, build native **SwiftUI** clients talking cleanly to the backend/data boundary (or Iroh/local bridge, as in `house-remodel/ios`), rather than Kotlin Multiplatform (KMP) or hybrid mobile wrappers.
* **If Peer / Admin Transport → Reach for Iroh:** Prefer authenticated **Iroh** connections or loopback processes fronted by Cloudflare Tunnel + Access.

---

# First Make it Work

## Laziness Protocol

* One decision lives in one place.
* Ship the smallest edit that solves the problem.

## Foundational Thinking

* Get the data shape right before you write the logic.
* Do first whatever helps every later phase.
* If a concurrent writer would matter, do not share state.

## TDD

* Write the failing test before you change the code.
* See it fail for the reason you expect.
* Make the smallest change that makes it pass.
* If an honest test is impractical, do not write a bad one.

## Redesign From First Principles

* Do not bolt a new requirement onto the old design.
* Ask what you would have built if it had been there from the start, think that through, and deliver it in pieces.

## Attack the Premise

* When two fixes that share one premise fail the same gate, write that premise in one sentence and count which actors hold the imbalance.
* Remove the assignment.
* Do not start another fix until you have done that.

## Subtract Before You Add

* Take complexity out before you add anything.

## Outcome-Oriented Execution

* Aim at the end state you can check.
* A break in the middle is allowed when you planned it and can undo it.

## Build the Lever

* If the work is not obvious at a glance, build the tool that does it.
* Do one unit by hand, then make the tool match that unit.

## Architecture

### Model the Domain

* Put the domain in a structure that makes the illegal states impossible.
* Maintain a glossary of domain terms.
* If the code is already clear and local, leave it.

### Boundary Discipline

* Check the data where it enters.
* Inside, trust it.
* Keep the business logic in a pure function.

### Type System Discipline

* Make the illegal value impossible to write.
* Parse outside data once, where it comes in.
* Do not silence the compiler.

### Make Operations Idempotent

* Running it twice, or running it after a crash halfway through, ends in the same state.

### Separate Before Serializing Shared State

* If they do not need the same mutable thing, give each its own.
* If they do, allow one writer, and enforce that in the structure.

# Then Make it Right

## Fix Root Causes

* Reproduce it.
* Fix the cause.
* A guard that only hides the crash is not a fix.

## Sequence work into verifiable units

* Finish one small unit and check it.
* Then start the next.

## Test Behavior, Not Implementation

* The test calls the code the way its user does and checks the result that user would see.
* If the test would still pass when every function returned nothing, throw it out.

## Crapper

* Use the crapper tool (https://github.com/unclebob/crapper — supports Clojure, Rust, TypeScript, Python, Go, Java).
* Reduce each function under CRAP 10 by splitting it, by testing it, or both.
* Consider whether case statements are worth reducing.

## Dryer

* Use the dryer tool (https://github.com/unclebob/dryer) to eliminate duplication that ranks 0.8 or above.

## Mutator

* Use the mutator tool (https://github.com/unclebob/mutator).
* Cover the uncovered.
* Kill the survivors.
* Delete dead code.

## Refactoring

* Pin what the code does.
* Change the structure.
* Prove the behavior did not change.

# Practice

## Keep a Log & Record Telemetry

* Keep a log of every action you take and the reason you took it.
* Report that log upon completion.
* Record each completed run and its measured gates into the global skills SQLite store by running:
  `python3 /workspace/skill-monitor/record_skill_run.py --skill ub-mode --agent "<Agent Name>" --target "<repo-or-task>" --outcome "<passed|failed>" --crap-max <val> --dry-max <val> --mutation-score <val> --compliant <1|0> --notes "<what changed and why>"`

## Guard the Context Window

* Keep the main thread small.
* Send the bulk elsewhere, and keep the summary.

## Worktree Cleanup

* Find what is safe to remove, confirm nothing still in use needs it, then reclaim the disk.
* Uncommitted work waits for a decision.

## Commit and Push

* Commit often, in small ordered commits.
* Each commit says why, what changed, and how you proved it.
* Push to the main line at the end.
