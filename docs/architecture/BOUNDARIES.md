# tsz Architectural Boundaries

**Version**: 2.0  
**Status**: Binding Boundary Policy  
**Last Updated**: March 2026

---

## 1. Purpose

This document defines the architectural boundaries that enforce the `NORTH_STAR.md` target architecture.

`NORTH_STAR.md` answers **where the architecture should converge**.  
This document answers **what code is allowed to know, own, import, mutate, and emit on the way there**.

If `NORTH_STAR.md` says **what we are aiming for**, this document says **what must be kept separate so we can actually get there**.

When code crosses a boundary for convenience, the result is usually one of these failures:

- the same semantic question gets answered through multiple routes,
- ambient state becomes required to understand meaning,
- caches become unsound or get bypassed,
- speculation leaks,
- diagnostics start encoding semantics,
- large-repo and incremental work become harder.

The rule is simple:

> Every layer should own one kind of truth. If a layer has to reach across a boundary, it should do so through an explicit request/response surface, not by sharing internals.

---

## 2. The Boundary Model in One Page

### 2.1 Layer Order

```text
source -> scanner -> parser -> binder -> checker -> solver -> emitter
                                      \-> diagnostics
                                      \-> lsp / project APIs
```

### 2.2 Allowed Direction of Knowledge

- **Scanner** may know text and tokenization.
- **Parser** may know syntax structure and recovery.
- **Binder** may know declarations, scopes, and stable semantic identity inputs.
- **Checker** may know source context, traversal order, and when to ask semantic questions.
- **Solver** may know semantic facts, relations, inference, and compatibility policy.
- **Emitter** may know checked semantic products and output shape.

### 2.3 Forbidden Direction of Knowledge

- Parser must not own semantic compatibility behavior.
- Binder must not own type inference or relation logic.
- Checker must not own shadow semantic kernels that compete with the solver.
- Solver must not own source-span policy or direct diagnostic rendering.
- Emitter must not perform semantic validation.

---

## 3. Canonical Ownership Table

| Layer | Owns | Must Not Own |
|---|---|---|
| Scanner | tokenization, trivia, escape handling, text scanning | parser recovery policy, semantic meaning |
| Parser | syntax trees, parse recovery, syntactic diagnostics | scopes, symbols, type semantics |
| Binder | declarations, scopes, symbol tables, stable semantic identity inputs, control-flow skeletons | type inference, assignability, compatibility policy |
| Checker | AST traversal, request construction, source-context decisions, suppression policy, diagnostics orchestration | competing semantic kernels, low-level solver internals, diagnostic rendering as semantics |
| Solver | semantic facts, type relations, inference, property presence, freshness, call compatibility, semantic caches | AST span policy, source-text specific rendering, suppression policy |
| Emitter | JS / `.d.ts` output generation from checked facts | type validation, semantic compatibility, late semantic discovery |
| LSP / Project | orchestration, project graph, incremental invalidation, workspace APIs | duplicate compiler semantics |

If code is doing both **semantic fact production** and **source anchoring / presentation**, the boundary is wrong.

---

## 4. Identity Boundary

Stable semantic identity is the first non-negotiable boundary.

### 4.1 Rules

1. **Binder-owned identity comes first.**
   Stable declaration/scope identity must be minted early and reused.

2. **AST handles are not semantic identity.**
   `NodeIndex` and similar syntax handles are traversal coordinates, not durable semantic keys.

3. **Checker must not invent semantic identity in hot paths.**
   If a semantic thing needs a stable identity, that identity should exist before checker hot paths recover it ad hoc.

4. **Cross-file work must key off stable semantic IDs.**
   Cross-file semantic reuse must not depend on transient arena position or raw syntax reachability.

5. **Lazy semantic references must be explicit.**
   Deferred semantic work should use explicit lazy semantic references, not hidden symbol/node reconstruction.

### 4.2 Consequences

- `SymbolId` / `DefId` / `TypeId`-style identities are first-class architectural tools.
- Cross-file repair maps are migration debt, not target architecture.
- A boundary is suspect if semantic work is keyed primarily by syntax handles when a stable semantic identity exists.

---

## 5. Request Boundary

Explicit requests are the second non-negotiable boundary.

### 5.1 Rule

If a semantic answer changes based on context, that context must cross the boundary as data.

### 5.2 Canonical Request Types

The exact names may evolve, but the architecture should converge on request types like:

- `TypingRequest`
- `RelationRequest`
- `DiagnosticRenderRequest`
- other small derived request types where narrower surfaces are useful

### 5.3 Rules

1. No ambient mutable context as the primary transport mechanism.
2. No hidden global "current request".
3. No request stack on a giant context object as the default architecture.
4. No save/mutate/restore of contextual state when a request can be passed.
5. Functions whose meaning changes under context should accept a request or a clearly derived sub-request.

### 5.4 Examples

Good:

```text
checker -> build TypingRequest -> query boundary -> solver answer
```

Bad:

```text
checker mutates ctx.contextual_type
checker calls helper
helper reads ambient field
helper mutates it again
caller restores old value later
```

---

## 6. Query Boundary: Checker ↔ Solver

This is the most important operational boundary in the compiler.

### 6.1 Checker Owns

- where the question came from,
- which request to build,
- when semantic work should happen,
- how to suppress or defer source-facing diagnostics,
- how to map structured failures to source locations.

### 6.2 Solver Owns

- what the semantic result is,
- what the relation result is,
- what property presence means,
- what object-literal freshness means,
- what call compatibility means,
- what type inference and evaluation produce,
- what structured semantic failure reason applies.

### 6.3 Rules

1. Checker must depend on solver through **canonical query surfaces**, not through ad hoc direct solver internals.
2. Query boundary inputs must be **stable identities + explicit requests + local source context handles**.
3. Query boundary outputs should be **semantic facts or structured semantic failures**, not pre-rendered diagnostics.
4. Checker must not routinely pattern-match on low-level solver internals to finish semantic work locally.
5. A checker helper that answers a semantic question differently from the solver is architectural debt unless explicitly temporary.

### 6.4 Automated Enforcement

The following contract tests in `crates/tsz-checker/tests/architecture_contract_tests.rs` enforce Query Boundary rules at the source level:

- **`test_no_direct_type_queries_data_access_outside_query_boundaries`**: Checker code outside `query_boundaries/` must not call `tsz_solver::type_queries::data::` functions directly. These internal data accessors must be wrapped as thin boundary helpers in `query_boundaries/common.rs`.
- **`test_no_direct_relation_policy_construction_outside_query_boundaries`**: `RelationPolicy` and `RelationContext` must only be constructed inside `query_boundaries/` where checker-level concepts are translated to solver-level knobs.
- **`test_direct_call_evaluator_usage_is_quarantined_to_query_boundaries`**: `CallEvaluator` must stay in query boundaries.
- **`test_control_flow_avoids_direct_union_interning`**: Control flow must not intern union types directly.
- **`test_no_direct_type_evaluator_construction_outside_query_boundaries`**: `TypeEvaluator` must only be constructed inside `query_boundaries/`. Checker code should use boundary wrappers: `evaluate_type_with_resolver` (simple), `evaluate_type_with_cache` (with cache seeding/draining), or `evaluate_type_suppressing_this` (heritage merging).
- **`test_no_direct_type_data_pattern_matching_outside_query_boundaries`**: Checker code must not pattern-match on `tsz_solver::TypeData` variants directly. Use solver query functions wrapped through `query_boundaries/` for type classification.

New contract tests should be added as new boundary rules are enforced.

### 6.5 What Belongs Behind the Query Boundary

The following behaviors should converge on query-boundary/solver ownership:

- assignability,
- call compatibility,
- property presence,
- missing-property classification,
- object-literal freshness and excess-property rules,
- property/element access semantics,
- inference and instantiation,
- semantic summaries for hot repeated work.

### 6.6 What Does Not Belong Behind the Query Boundary

The solver should not:

- choose source anchors,
- decide related-info ordering for diagnostics,
- suppress diagnostics for user-facing reasons,
- depend on checker ambient context,
- depend on direct AST traversal as the normal semantic interface.

---

## 7. Relation Policy Boundary

The compiler must have one authoritative compatibility policy surface.

### 7.1 Rule

If TypeScript compatibility requires a policy choice, that policy should still be centralized.

### 7.2 This Means

The following must not be answered through separate competing local routes:

- assignability in variable assignment,
- assignability in call arguments,
- fresh object-literal excess-property checking,
- missing-property classification,
- property presence in unions/intersections,
- property access semantics,
- write-vs-read access semantics where the answer changes.

### 7.3 Anti-Pattern

A fix that makes `TS2322`, `TS2339`, or `TS2345` better in one path but worse in another usually means the same semantic question is still being answered in more than one place.

---

## 8. Diagnostics Boundary

Diagnostics are downstream of semantics.

### 8.1 Layer Split

1. **Semantic fact**
2. **Structured failure explanation**
3. **Rendering / anchor / formatting policy**

### 8.2 Rules

1. The solver should return semantic facts and structured reasons, not formatted diagnostics.
2. The checker/error-reporter layer should render diagnostics from structured reasons.
3. Reporters must not encode semantic policy that competes with the solver.
4. Anchor policy must be centralized and reusable.
5. Related-info ordering and deduplication must be policy-driven.
6. A diagnostic-only fix must not change semantic outcomes.

### 8.3 Consequences

- “Wrong place, right code” is usually a rendering-layer bug.
- “Right place, wrong code” is usually a semantic-path bug.
- A diagnostic pass must not become a hidden semantics fork.

---

## 9. Speculation Boundary

Speculative work is allowed. Leaky speculative state is not.

### 9.1 Speculation Includes

- overload probing,
- contextual experimentation,
- dead-branch or conditional probing,
- tentative generic inference,
- return-type inference experiments,
- JSX candidate exploration,
- any semantic probe whose results may be discarded.

### 9.2 Rules

1. Speculation must occur inside an explicit transaction boundary.
2. Diagnostics, dedup state, and request-sensitive caches must roll back unless committed.
3. Open-coded clone/truncate/restore logic is migration debt.
4. Cache rollback belongs in shared transaction infrastructure, not caller discipline.
5. Query boundaries used during speculation must support rollback semantics where relevant.

### 9.3 Anti-Pattern

A speculative path that mutates global checker state and then “carefully restores it” is not the target architecture.

---

## 10. Cache Boundary

Caching is a correctness boundary, not just a speed trick.

### 10.1 Ownership Split

- **Checker-owned caches**: AST-local traversal caches, source-context caches, request-sensitive expression caches, diagnostic shaping helpers
- **Solver-owned caches**: type interning, relation memoization, semantic summaries, evaluation caches

### 10.2 Rules

1. Request-free results must not be confused with request-sensitive results.
2. Request-sensitive caches must use explicit request-aware keys.
3. Speculative work must not leak cache entries unless committed.
4. Cache keys must be explicit and auditable.
5. Recursive cache clearing is not a normal semantic control-flow tool.
6. Cache ownership must be visible from the module structure.

### 10.3 Anti-Patterns

- blanket “request non-empty => bypass cache forever” logic,
- cache keys that silently omit dimensions that affect meaning,
- cache clearing as the standard way to re-run a computation,
- caches that depend on ambient mutable context to remain sound.

---

## 11. State Boundary

The compiler should not converge on one giant mutable session object.

### 11.1 Preferred Separation

The architecture should trend toward separate state buckets such as:

- `ProjectSemanticState`
- `FileCheckSession`
- `CheckScratch`
- `TypingRequest` / `RelationRequest`
- `DiagnosticTxn` / `DiagnosticSink`

### 11.2 Rules

1. Project-wide state, file-local state, request state, speculative state, diagnostic state, and cache state should not all mix freely in one mutable bag.
2. Mutable state that must roll back should be easy to identify.
3. Request state should not live as hidden ambient mutable fields.
4. Long-lived state should be distinguishable from per-check scratch.

### 11.3 Migration Note

A large `CheckerContext` can exist during migration. It should steadily shrink in responsibility, not become the permanent architecture.

---

## 12. Parser / Binder / Checker Boundaries

### 12.1 Parser Boundary

Parser owns syntax and recovery only.

Rules:

- parser diagnostics should be syntactic,
- parser recovery should not depend on semantic knowledge,
- parser should not import relation or type-checking logic.

### 12.2 Binder Boundary

Binder owns declarations, scopes, symbol tables, and identity inputs.

Rules:

- binder must not import semantic relation logic,
- binder should produce stable semantic skeletons rather than forcing checker-side identity recovery,
- control-flow skeleton creation belongs closer to binder/checker boundary than to solver semantics.

### 12.3 Checker Boundary

Checker owns traversal, source context, query construction, and diagnostics orchestration.

Rules:

- checker should be thin in semantics and rich in source awareness,
- checker helpers should not become shadow semantic kernels,
- checker should depend on canonical boundary APIs instead of internal solver details.

---

## 13. Emitter Boundary

Emitter consumes checked facts and produces output.

### 13.1 Rules

1. Emitter must not perform new semantic validation.
2. Emitter may depend on stable semantic summaries and checked products.
3. Declaration emit naming/accessibility logic should prefer precomputed semantic summaries over late semantic rediscovery.
4. JS emit and declaration emit should not need checker-local semantic reinvention.

### 13.2 Consequence

When emit behavior requires large amounts of fresh semantic rediscovery, the semantic boundary is usually too weak upstream.

---

## 14. Large-Repo / Incremental Boundary

Large-repo support depends on stable identity and bounded residency.

### 14.1 Rules

1. Project-scale indexing should be keyed by stable semantic identity.
2. Incremental invalidation should prefer semantic/API fingerprints over broad whole-project churn.
3. AST residency should not be a permanent assumption for semantic reuse.
4. Long-lived project APIs should consume summaries and identities, not borrow compiler internals directly.
5. A scheduler is not the first milestone; stable identity and bounded semantic products are.

### 14.2 Consequence

Any design that makes incremental work depend on permanent whole-world AST reachability is moving away from the target architecture.

---

## 15. Migration Rules

This repo is mid-migration. Some transitional code exists. Transitional code must still obey these rules.

### 15.1 Rules

1. Transitional code must point toward the target architecture, not normalize the old one.
2. Temporary wrappers should shrink direct boundary violations, not hide them forever.
3. Allowlists and architecture tests are acceptable only when they have a clear ratchet direction.
4. A temporary exception should be specific, visible, and removable.
5. “This is faster for now” is not enough justification for permanent boundary erosion.

### 15.2 Review Question

Does this change remove a transitional layer, or merely move it?

If it only moves it, the PR should say so explicitly.

---

## 16. Enforcement

### 16.1 Primary Enforcement

Prefer enforcement through:

1. crate/module visibility,
2. narrow public APIs,
3. explicit request/response surfaces,
4. transaction infrastructure,
5. typed cache keys and ownership.

### 16.2 Secondary Enforcement

Use architecture tests and scripts to catch:

- forbidden imports,
- ambient mutable context writes,
- direct solver-internal access from checker paths,
- open-coded speculation restore patterns,
- bespoke diagnostic anchor logic after centralization,
- blanket cache bypass regressions.

### 16.3 Important Principle

Architecture tests are valuable. They are not the end-state architecture.

The ideal is that the wrong move is structurally difficult, and the tests exist to catch regressions and migration leaks.

---

## 17. Practical Review Checklist

When reviewing a non-trivial PR, ask:

1. Did this change add a new semantic path or remove one?
2. Did it make context more explicit or more ambient?
3. Did it strengthen stable identity or recover it later?
4. Did it move logic behind a canonical query surface or reach across the boundary more directly?
5. Did it separate semantics from diagnostics, or mix them more tightly?
6. Did it make speculative work more transactional or more caller-disciplined?
7. Did it improve cache ownership or add another bypass/clear path?
8. Did it make the wrong architectural move harder in code, not just less polite?

---

## 18. Anti-Goals

The following are explicitly outside the intended boundary model:

1. Re-implementing solver semantics in checker helpers.
2. Passing meaning through ambient mutable state.
3. Letting diagnostics choose semantics.
4. Treating cache bypass as the permanent answer to cache unsoundness.
5. Treating recursive cache clearing as normal semantic control flow.
6. Making emitter or parser responsible for semantic compatibility.
7. Building large-repo scheduling before stable identity and bounded semantic products exist.
8. Relying on grep-style architecture tests alone when module visibility could enforce the rule directly.

---

## 19. Final Rule

When there is uncertainty, choose the design that preserves these boundaries:

- **identity is stable and early**,
- **requests are explicit**,
- **checker asks, solver answers**,
- **semantics and diagnostics stay separate**,
- **speculation is transactional**,
- **caches are explicit and auditable**,
- **large-repo work depends on summaries and identities, not ambient whole-world reachability**.

If a design violates one of these boundaries, it should be treated as temporary debt unless proven otherwise.
