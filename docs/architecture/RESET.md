# Clean-Slate Compiler Architecture

This document is the durable architecture contract for the TSZ rewrite. The
execution order and gates live in `docs/plan/ROADMAP.md`.

## Design Bias

TypeScript compatibility is path-dependent. The new compiler should resemble
the pinned TypeScript 7 implementation closely enough that an upstream behavior
has one obvious home and can be ported with its preconditions intact. Rust data
layout may differ; semantic sequencing must not drift casually.

The old architecture split semantic work between a checker, a general solver,
query-boundary adapters, two type environments, and many recovery paths. The
replacement keeps ownership but removes competing semantic engines.

## Module Graph

```text
syntax ──> program ──> checker ──> emit
               │          │
               └──────────┴─────> service ──> CLI / server / LSP / WASM
```

- `syntax`: tokens, trivia, immutable AST arenas, parser recovery, source maps.
- `program`: compiler options, normalized paths, source loading, resolution,
  root order, declaration index, and immutable project facts.
- `checker`: binding/scopes/flow, type creation, inference, relation,
  contextual typing, semantic queries, and structured failures.
- `emit`: syntax transforms and printing; DTS consumes explicit semantic summaries.
- `service`: the only public compiler/project/language-service facade.

The binder may become a separate module as it grows. It does not become a
separate mutable semantic universe.

## Identity

Use compact handles whose domain is explicit:

- `FileId`: index into the program's path-sorted source table.
- `NodeId`: index into one immutable file arena; pair it with `FileId` outside that arena.
- `DeclId`: program-owned `(FileId, NodeId)` declaration identity.
- `SymbolId`: program-local merged-symbol identity with an ordered declaration list.
- `TypeParamId`: declaration/binder-scoped identity, never name-only.
- `TypeId`: checker-session-local handle into one type store.

Rules:

1. The binder/program assigns declaration and symbol identities once.
2. Content hashing may deduplicate immutable bytes, not elect nominal identity.
3. Two checker sessions never compare, persist, or exchange `TypeId` values.
4. Incremental products use declarations and API fingerprints, not stale type handles.
5. Built-ins resolve through program-owned declarations, never user spelling tests.

## Syntax And Program Construction

Scanning and parsing are file-local and recover to immutable syntax plus syntax
diagnostics. Program construction normalizes and sorts root paths, resolves the
module graph, freezes source order, then builds declarations/scopes.

No semantic work starts while declarations are still being mirrored or merged.
Parallel scanning/parsing/binding is permitted only because each task writes to
isolated file storage and the program performs a deterministic merge afterward.

## Type Representation

The initial type store contains primitives and ordinary structural types, but
its shape reserves first-class symbolic forms:

- declaration reference and application;
- type parameter and inference placeholder;
- indexed access and `keyof`;
- conditional and mapped type;
- union/intersection with an explicit reduction policy;
- object, tuple, signature, literal, template, and unique/nominal forms.

Interning canonicalizes an already-chosen representation. It does not decide
whether a union should be subtype-reduced, force an application, resolve an
indexed access, or erase alias/provenance information.

## Queries And Completion

Semantic requests carry all context that can affect the result. Examples:

- typing position and contextual type;
- relation kind, variance, freshness, and compatibility flags;
- inference priority/source and mapper identity;
- target/module/options;
- active recursion identities and limits.

Operations return a semantic value together with:

```text
Complete | Deferred | Cycle | Limit
```

`Deferred`, `Cycle`, and `Limit` are not errors and are not types. Callers must
propagate or handle them according to the ported TypeScript operation. There is
one gateway for apparent type, property lookup, symbolic dispatch, and any
required materialization; callers do not invent local force rules.

## Inference And Relation

Inference and relation inspect compatible symbolic pairs before materializing.
For example, indexed-access sources and targets compare their symbolic object
and index operands as TypeScript 7 does. Recursion containment counts identities
of the active relation/evaluation, not only repeated concrete `TypeId` pairs.

A relation returns a structured tree such as missing property, incompatible
property, signature mismatch, literal mismatch, or indeterminate completion.
Diagnostic rendering never feeds back into the relation.

## Speculation

Overload probing, contextual typing, JSX candidate selection, and conditional
branch probing use an explicit transaction. Scratch types, inference state,
diagnostics, and request-sensitive memo entries roll back together unless the
transaction commits. No open-coded clone/truncate/restore protocol is allowed.

## Caching

Start uncached. Add memoization only after the semantic operation is correct and
the key is reviewable.

Every cache declares:

- the exact semantic question;
- all key fields and their domains;
- whether incomplete answers are representable (normally they are not cached);
- dependency/invalidation behavior;
- request/session lifetime and residency bound;
- cold/warm, enabled/disabled, file-order, and repeated-run agreement tests.

Pure type interning is not a semantic-result cache. It still remains local to a
checker session and cannot perform hidden reduction.

## Concurrency

The reference compiler service uses one checker. This avoids cross-checker type
mixing and makes determinism observable. Later checker pools may assign files to
independent checker universes, following TypeScript 7, only if:

- no raw type crosses the pool boundary;
- diagnostics/text/summaries are the only merged outputs;
- one checker is never accessed concurrently;
- repeated 1/N-thread results have identical diagnostic fingerprints;
- memory duplication remains bounded and measured.

## Diagnostics And Emit

Diagnostics are sorted and deduplicated once at the program boundary. Their
identity includes code, normalized path, span, message chain, and related info.
Pretty/non-pretty formatting is a terminal presentation step.

JavaScript emit operates on syntax. Declaration emit consumes explicit checked
summaries and source provenance. Neither output path performs general semantic
validation or reparses rendered output.

## Public Boundary

`service` exposes source/config inputs and stable products:

- diagnostics;
- emitted files;
- immutable syntax/symbol queries;
- language-service responses;
- phase timings and bounded counters.

CLI, framed server, LSP, and WASM adapt this API. They do not construct an
alternate compiler pipeline.

## Enforcement

Prefer Rust visibility and types over grep allowlists:

- session-local handles cannot be serialized or exported;
- incomplete completion cannot convert implicitly to a type;
- emit cannot import checker scratch state;
- CLI depends only on the service API;
- compiler files remain below 2,000 physical lines;
- architecture tests compile invalid examples where practical, then use small
  source scans only for anti-hardcoding and size rules.

The architecture is succeeding when adding a TypeScript behavior has one
obvious module, one oracle-backed algorithm, one identity domain, and no new
ambient flag, mirrored store, force path, or cache exception.
