---
id: migration-vocabulary
layer: core
status: active
created: 2026-07-14
revised: 2026-07-14
tags: [mct, patina, migration, vocabulary, mother, child, toy]
references: [what-is-mct, mct-build-boundaries, mct-patina-migration]
---

# MCT and Integrated Patina Migration Vocabulary

**Purpose:** Keep comparative work unambiguous while responsibilities move
from the integrated Patina architecture into MCT. This is a transitional
communication convention, not a request to rename Rust types, protocols,
Allium entities, or ordinary MCT terminology.

Semantic law: `layer/allium/mct-patina-migration.allium`. This document explains
how to apply that law in prose. The Allium contracts govern meaning; the CI
vocabulary check catches ambiguous terms in living comparative documents.

## Context determines the vocabulary

In MCT-only discussion, use the ontology's ordinary terms:

```text
Mother
Child
Toy
```

Do not rewrite canonical MCT prose or code to qualify every role. Phrases such
as "MCT Mother" are redundant in ordinary English when only MCT is in frame.

When MCT and the integrated Patina architecture are compared in the same
conversation, report, issue, specification, or pull request, use these
camelCase namespace tokens:

| Token | Meaning |
|---|---|
| `mctMother` | The Mother role and implementation being built in MCT |
| `mctChild` | A Child governed by `mctMother` |
| `mctToy` | An authority-granted capability in MCT |
| `patinaMother` | The resident daemon in the integrated Patina architecture |
| `patinaChild` | The integrated Patina daemon's legacy Child model/runtime |
| `patinaToy` | The integrated Patina manifest-driven capability model |

The camelCase terms are namespace qualifiers, not product names and not prose
such as "MCT Mother." They exist only to disambiguate comparative work.

## Applications keep their names

A Child is a role; it is not part of an application's brand.

- Slate is Slate. It operates as an `mctChild`.
- If Patina becomes a Child, it remains Patina and operates as an `mctChild`.
- Do not rename applications to `slateChild`, `patinaApp`, or similar forms
  merely to expose their runtime role.
- `patinaChild` always means the legacy Child model in comparative work. It
  does not name the future belief-centered Patina application.

## Parallel labels do not imply equivalent authority

The `patinaToy` and `mctToy` labels identify concepts from different systems;
they do not claim that the concepts have equal semantics or legitimacy.

A `patinaToy` is based on the integrated Patina capability model, where
manifest needs can become runtime grants. An `mctToy` belongs to the closed
MCT capability catalog and requires explicit, scoped, revocable ToyGrant
authority. Translating a use case from `patinaToy` to `mctToy` therefore
requires a new authority design. The parallel names must never justify copying
allow-by-default or ambient capability behavior.

## Migration verbs

Use verbs as audit signals:

| Verb | Required meaning |
|---|---|
| **translate** | Carry a concept or requirement across while allowing its ontology and implementation shape to change |
| **rebuild** | Implement an accepted responsibility under MCT authority, lifecycle, and observation law |
| **port** | Intentionally reuse implementation code; the work must justify why the source shape is safe and appropriate |
| **replace** | `mctMother` takes over an operational responsibility previously performed by `patinaMother` |
| **retire** | Remove a legacy responsibility or implementation after replacement and migration are complete |

Do not use **port** as a synonym for adopting an idea. Existing code may be
behavioral evidence while still being unsuitable for reuse.

## Examples

Preferred:

> We are evaluating `patinaMother` child warmup to decide which operational
> responsibility `mctMother` must rebuild.

> Slate is an `mctChild` whose effects require explicit `mctToy` grants.

> The network use case demonstrated by `patinaToy` must be translated into a
> deny-by-default `mctToy`; its adapter should not be ported.

Avoid:

> MCT Mother should port Mother's toy system for the Patina child.

The avoided sentence is ambiguous about both architectures, overuses Mother,
and incorrectly treats **port** as equivalent to redesign.

## Sunset condition

This convention remains active while `patinaMother`, `patinaChild`, and
`patinaToy` are live migration subjects. After the integrated architecture is
retired and comparative ambiguity is gone, remove the transitional qualifiers
and return to ordinary Mother/Child/Toy vocabulary everywhere.
