# tsz North Star Architecture

Status: target architecture for substantial compiler work.

This document says what tsz must converge toward. The living execution plan is
`docs/plan/ROADMAP.md`; layer guardrails live in `docs/architecture/BOUNDARIES.md`.

## One-Sentence Target

tsz should produce the same project result as `tsc`, substantially faster when
it succeeds, by running all type semantics through one solver-owned semantic
substrate with stable identity, explicit requests, transactional speculation,
and auditable caches.

## Core Principles

1. **Solver owns semantics.** Relations, evaluation, inference, instantiation,
   narrowing, type traversal, compatibility policy, and semantic classification
   belong in the solver or query-boundary helpers.
2. **Checker owns source context.** The checker decides when and where to ask a
   question, tracks source locations, and renders diagnostics from structured
   semantic failures. It must not become a shadow solver.
3. **Binder owns identity.** Symbols, scopes, flow skeletons, declaration facts,
   and stable semantic identities are binder/project facts, not recovered ad hoc
   in checker hot paths.
4. **Emitter owns output.** JS and declaration emit consume syntax plus checked
   semantic summaries. Emitters do not validate types on the fly.
5. **Consumers consume.** CLI, LSP, WASM, and website tooling should converge on
   one compiler service surface instead of building parallel compiler pipelines.

## Pipeline

```text
scanner -> parser -> binder -> checker -> solver -> emitter
                                  -> diagnostics
                                  -> compiler service -> CLI/LSP/WASM
```

The ownership shorthand is:

- **Binder = WHO**: what declaration/symbol/flow entity exists.
- **Solver = WHAT**: what a type, relation, evaluation, or narrowing means.
- **Checker = WHERE**: where in source a question is asked and reported.
- **Emitter = OUTPUT**: what text/artifacts are produced.

If a path computes both `WHAT` and `WHERE`, it is probably wrong.

## Stable Identity

Semantic identity must be stable before hard semantic work begins.

Canonical handles:

- `Atom`: interned text identity.
- `SymbolId`: binder-owned symbol/declaration identity.
- `FlowNodeId`: binder-owned flow identity.
- `DefId`: stable semantic definition identity.
- `TypeId`: solver-canonical type identity.
- `NodeIndex`: syntax traversal coordinate, not cross-file semantic identity.

Rules:

1. Semantic references use stable IDs, preferably `Lazy(DefId)` at solver
   boundaries.
2. Cross-file and incremental reuse are keyed by stable semantic identity, not
   transient arena position.
3. Checker code does not invent identity in hot paths when binder/project state
   can provide it earlier.
4. Built-in and lib concepts resolve through binder/global tables or builtin
   IDs, not string matching over user spelling.

## Explicit Requests

Any semantic answer that can change with context must receive that context as
data.

Examples:

- `TypingRequest`: contextual type, flow intent, expression-origin data.
- `RelationRequest`: assignment/argument/return/write mode, freshness,
  compatibility mode, variance, strictness flags.
- `DiagnosticRenderRequest`: anchor and display policy after the semantic
  failure is already known.

Rules:

1. No hidden global "current request" as the primary mechanism.
2. No save/mutate/restore ambient state when a request object can be passed.
3. Request-sensitive results require request-sensitive cache keys or an explicit
   bypass with a documented fallback.

## Relation Policy

The compatibility stack has two roles:

- **Judge**: strict structural/set-theoretic relation.
- **Lawyer**: TypeScript compatibility exceptions such as `any` propagation,
  variance modes, object-literal freshness, weak types, and `void` return
  exceptions.

Assignability, call compatibility, property presence, missing-property
classification, excess-property checking, and write/read property behavior must
converge on one authoritative relation/query gateway.

For `TS2322`, `TS2345`, `TS2394`, `TS2416`, and related diagnostics, the order
is fixed:

```text
relation -> structured reason -> diagnostic rendering
```

## Advanced Type Substrate

Advanced TypeScript support depends on a robust evaluator and instantiation
model, not local patches.

The solver must own:

- conditional and distributive evaluation,
- mapped types and key remapping,
- template literal types and inference,
- `infer` variables and constraint flow,
- indexed access and `keyof`,
- type application and instantiation,
- recursive/cyclic type handling,
- narrowing over semantic type facts,
- memoization/fuel/cycle policy.

The checker may request these answers. It must not reconstruct them by matching
printer output, user-chosen names, or raw internals.

## Diagnostics

Diagnostics are downstream of semantics.

1. Decide the semantic fact.
2. Produce a structured failure reason.
3. Render code, message, source span, and related information.

Reporter or printer code must not secretly decide type semantics. Printer output
is an output, never an input to compiler decisions.

## Speculation

Speculative work is normal: overload probing, contextual typing experiments,
conditional branch probing, JSX candidate exploration, and tentative generic
inference all need it.

Rules:

1. Speculation has an explicit transaction boundary.
2. Diagnostics, request-sensitive caches, and scratch state roll back unless
   committed.
3. Rollback behavior belongs in shared infrastructure, not caller discipline.

## Caches

Caches are correctness architecture.

Rules:

1. Cache keys include every input that can change the answer.
2. Relation/evaluation/inference/instantiation caches include mode and request
   context where relevant.
3. Cache-enabled and cache-disabled behavior should agree on targeted semantic
   tests.
4. Recursive cache clearing and permanent bypasses are migration smells, not the
   target design.
5. Speculative cache entries must not leak unless the transaction commits.

## Large Projects And Incremental Work

Large-repo readiness depends on stable semantic skeletons and bounded residency.

Prefer:

- stable declaration summaries,
- skeleton-derived project indexes,
- semantic queries over stable IDs,
- bounded AST/binder arena retention,
- invalidation by semantic/API fingerprint.

Do not trade semantic correctness for hot-path shortcuts. Performance wins must
preserve the canonical semantic substrate.

## Review Gate

For every non-trivial change, ask:

1. Does this reduce semantic paths or create another one?
2. Is this `WHAT` work in solver/query boundaries, or `WHERE` work in checker?
3. Does context become more explicit or more ambient?
4. Does identity become more stable or get recovered later?
5. Does this centralize relation/evaluation/narrowing policy?
6. Are cache keys and speculation behavior auditable?
7. Does the fix state a structural rule rather than a single test symptom?
8. Does it keep diagnostics downstream of semantic facts?

When uncertain, choose the design with one semantic truth source, explicit
requests, stable identity, transactional speculation, auditable caches, and
diagnostics downstream of solver facts.
