# tsz North Star Architecture

**Version**: 2.0  
**Status**: Binding Target Architecture  
**Last Updated**: March 2026

---

## 1. Purpose

This document defines the architecture that **all substantial refactoring work must converge toward**.

It is a target document, not a description of today's code. If the code and this document disagree, the code is carrying transitional debt unless there is an explicit, time-bounded exception recorded elsewhere.

The purpose of this document is to keep tsz moving toward a compiler that is:

- correct enough to match TypeScript closely,
- fast enough to beat competing implementations on real workloads,
- maintainable enough that major work reduces complexity instead of relocating it,
- scalable enough to handle large repositories without keeping the whole world alive in memory.

The core heuristic is simple:

> A good architectural change reduces the number of semantic paths, makes context explicit, makes identity stable, makes speculative work transactional, and makes caching more auditable.

---

## 2. The North Star in One Page

tsz should converge on these ideas:

1. **Stable semantic identity first**  
   Semantic work must run on stable identities minted early and owned centrally. We should not recover identity ad hoc in hot checker paths.

2. **Explicit requests over ambient mutable state**  
   Context that changes meaning must be passed explicitly as data, not installed into mutable globals and restored later.

3. **Solver owns semantics; checker owns source context**  
   The checker decides *where* and *when* to ask questions. The solver decides *what the semantic answer is*.

4. **One authoritative relation policy surface**  
   Assignability, call compatibility, property presence, object-literal freshness, and related compatibility behavior must not be encoded through several competing routes.

5. **Diagnostics are downstream of semantic facts**  
   The compiler should decide semantic fact, then failure reason, then rendering. Formatting and anchoring must not be where semantics secretly live.

6. **Speculation is transactional**  
   Overload probing, dead-branch evaluation, contextual experimentation, and other tentative work must roll back cleanly unless explicitly committed.

7. **Caches are part of the architecture, not incidental optimization**  
   Request-sensitive computations need request-sensitive caches. Cache invalidation, rollback, ownership, and scope must be designed, not improvised.

8. **Bounded residency and incremental identity are first-class goals**  
   Large-repo architecture should favor stable semantic skeletons, bounded memory, and incremental invalidation, not permanent AST residency.

These principles are not independent. They reinforce each other.

- Stable identity makes query boundaries and incremental work possible.
- Explicit requests make caching and rollback safe.
- Central relation policy reduces false positives, all-missing diagnostics, and drift.
- Diagnostic separation keeps semantics from leaking into rendering.

---

## 3. Canonical System Model

### 3.1 High-Level Pipeline

```text
source -> scanner -> parser -> binder -> checker -> solver -> emitter
                                      \-> diagnostics
                                      \-> project/lsp consumers
```

### 3.2 Layer Responsibilities

| Layer | Owns | Must Not Own |
|---|---|---|
| Scanner | Tokenization, text scanning, trivia, interned text handles | Parser recovery policy, semantic interpretation |
| Parser | Syntax tree construction, syntax recovery | Symbol identity, type semantics |
| Binder | Declarations, scopes, control-flow skeletons, stable semantic skeleton/input identity | Type inference, compatibility logic |
| Checker | AST traversal, request construction, source-context decisions, suppression policy, diagnostics orchestration | Ad hoc type algorithms, direct low-level solver internals |
| Solver | Semantic facts, relations, inference, evaluation, type traversal, compatibility policy | Source-span policy, AST-driven diagnostics |
| Emitter | Output generation from checked state | Semantic validation, type reasoning |
| LSP / Project | Orchestration, project graph, incremental invalidation, consumer APIs | Compiler-internal semantic duplication |

### 3.3 WHO / WHAT / WHERE / OUTPUT

- **Binder = WHO**: what declaration/symbol/flow entity exists
- **Solver = WHAT**: what the semantic meaning or relation result is
- **Checker = WHERE**: where in source the question came from and where to report it
- **Emitter = OUTPUT**: what files/text should be produced

If a piece of code is doing both **WHAT** and **WHERE**, it is a likely architectural smell.

---

## 4. Stable Identity Model

Stable identity is the foundation of the architecture.

### 4.1 Canonical IDs

- **`SymbolId`**: binder-owned declaration/scope identity
- **`DefId`**: canonical semantic definition identity used for semantic references and lazy resolution
- **`TypeId`**: canonical semantic type identity produced by the solver/interner
- **`NodeIndex` / AST node handles**: local syntax traversal handles, not semantic identity

### 4.2 Rules

1. New semantic references must be expressed through stable semantic identities, not recovered from syntax on demand.
2. Checker code must not invent semantic identity in hot paths when it could be minted earlier.
3. AST handles are traversal coordinates, not durable semantic keys.
4. Lazy semantic references should be explicit (`Lazy(DefId)`-style), not hidden behind incidental symbol indirection.
5. Cross-file and incremental work must be keyed by stable semantic identity, not transient arena position.

### 4.3 Why This Matters

Stable identity is what makes all of the following tractable:

- recursion and circularity handling,
- incremental invalidation,
- declaration emit naming,
- request-aware caching,
- AST eviction / bounded residency,
- large-repo project graphs,
- consistent semantic reuse across files.

When identity is weak, the checker becomes a repair layer. That is not the target architecture.

---

## 5. Explicit Request Model

### 5.1 Principle

If a semantic answer can change based on context, that context must be represented explicitly in a request object.

No ambient mutable fields should be required to know the meaning of a computation.

### 5.2 Canonical Requests

At minimum, tsz should converge on explicit request types like these:

- **`TypingRequest`**: contextual type, contextual origin, flow intent, other expression-meaning inputs
- **`RelationRequest`**: assignment vs argument vs return vs property read/write, freshness mode, compatibility mode, strictness knobs
- **`DiagnosticRenderRequest`**: anchor policy, exact-vs-rewritten anchor mode, related-info shaping

The exact names can evolve. The architecture principle should not.

### 5.3 Rules

1. No global “current request”.
2. No request stack hidden inside checker context as the primary mechanism.
3. No save/mutate/restore of ambient context when a request object can be passed.
4. Request-sensitive results must not be cached as if they were request-free.
5. A function that changes meaning under context should accept a request or a clearly derived sub-request.

### 5.4 Implication

The end-state is not “replace three ambient booleans with one ambient struct.”  
The end-state is **request-first APIs all the way down the relevant path**.

---

## 6. Semantic Ownership Model

### 6.1 Solver-First, But With a Better Precision

“Solver-first” means more than “put type operations in another crate.” It means:

- semantic relations are defined in one place,
- type traversal is solver-owned,
- compatibility policy is solver-owned,
- checker uses query boundaries and request objects,
- checker does not carry shadow semantics in local helper paths.

### 6.2 Relation Kernel and Compatibility Policy

tsz should maintain a clear distinction between:

- a **relation kernel** that computes semantic relations and facts,
- a **compatibility policy layer** that models TypeScript-specific behavior,
- a **checker rendering layer** that turns structured failures into diagnostics.

This can be described as “judge and lawyer” if useful, but the binding rule is simpler:

> If TypeScript compatibility requires a policy choice, that policy must still be centralized and solver-owned, not spread across call sites.

### 6.3 The Big Unification Goal

The following must converge on one authoritative semantic kernel:

- assignability,
- call compatibility,
- property presence,
- missing-property classification,
- fresh object-literal excess-property checking,
- property access on unions and intersections,
- write-vs-read property/element access behavior where semantics differ.

If these questions are answered through different local routes, tsz will keep producing the same families of missing and extra diagnostics.

---

## 7. Diagnostics Model

Diagnostics must be the product of three separate layers:

1. **Semantic fact**: what failed
2. **Failure explanation**: why it failed in a structured way
3. **Rendering policy**: how to anchor, label, and format it

### 7.1 Rules

1. Reporter code must not secretly decide semantics.
2. Anchor policy must be centralized, not scattered across unrelated reporters.
3. Related-information ordering and deduplication must be policy-driven.
4. Speculative paths must not leak committed diagnostics unless explicitly kept.
5. The same semantic failure should not render through materially different code paths depending on who asked the question.

### 7.2 Consequence

A “diagnostic fix” that changes semantics is not a real diagnostics fix.  
A “semantic fix” that depends on renderer-specific branching is also not a real semantic fix.

---

## 8. Transactional Speculation

Speculative work is normal in a TypeScript-compatible compiler. Leaky speculation is not.

### 8.1 Speculative Operations

Examples include:

- overload probing,
- contextual experimentation,
- conditional branch probing,
- tentative generic inference,
- return-type inference experiments,
- JSX candidate exploration.

### 8.2 Rules

1. Speculation must happen inside an explicit transaction boundary.
2. Diagnostics, dedup state, request-sensitive caches, and other speculative artifacts must roll back unless committed.
3. Rollback semantics belong in shared infrastructure, not caller discipline.
4. Open-coded clone / truncate / restore patterns are transitional debt.

### 8.3 Desired End-State

The normal shape of speculative code should be:

- begin transaction,
- do speculative work,
- either commit or drop,
- no manual state surgery at the call site.

---

## 9. Cache Architecture

Caching is a correctness concern as much as a performance concern.

### 9.1 Ownership Split

- **Checker-owned caches**: AST-local traversal artifacts, source-context artifacts, flow/query orchestration caches, diagnostic shaping caches
- **Solver-owned caches**: type interning, relation memoization, evaluation caches, semantic summaries

### 9.2 Rules

1. Request-free computations may use request-free caches.
2. Request-sensitive computations must use request-aware caches or explicit bypass with justification.
3. Speculative computations must not leak cache entries unless committed.
4. Cache keys must be explicit and audited.
5. Recursive cache clearing must not be standard semantic control flow.

### 9.3 Preferred Cache Strategy

The long-term shape should include:

- request-aware caches for request-sensitive expression typing,
- semantic summary caches for hot repeated work,
- bounded cache residency,
- invalidation by stable semantic identity.

### 9.4 Summary Caches We Should Prefer Over Repeated Re-Walks

Examples of desirable summary-style caches:

- optional-chain summaries,
- union-property summaries,
- class closure summaries,
- normalized inference-bounds summaries,
- stable declaration/visibility summaries for emit.

### 9.5 Anti-Pattern

`clear_type_cache_recursive`-style behavior is a warning sign.  
It may be necessary during migration, but it is not the target architecture.

---

## 10. Context and State Shape

The checker should not converge on one giant mutable session object.

### 10.1 Preferred State Split

The architecture should move toward distinct layers such as:

- **ProjectSemanticState**: global/shared immutable-ish semantic substrate and long-lived caches
- **FileCheckSession**: file-local facts and file-scoped cache/view state
- **CheckScratch**: transient mutable scratch for the current walk
- **TypingRequest / RelationRequest**: explicit semantic context
- **DiagnosticTxn / DiagnosticSink**: transactional diagnostic collection

Names can vary. The separation should remain.

### 10.2 Rule

A single `CheckerContext` should not be the place where all of the following mix freely:

- project state,
- file state,
- request state,
- diagnostic state,
- cache state,
- speculative state,
- migration compatibility state.

That shape is acceptable only as a temporary migration structure.

---

## 11. Boundary Enforcement

Architecture should be enforced by the code structure first, and by tests second.

### 11.1 Rules

1. Checker must depend on solver through canonical query surfaces, not direct internal construction.
2. Checker must not pattern-match on low-level solver internals as a normal practice.
3. Binder must not import semantic relation logic.
4. Emitter must not perform semantic validation.
5. Forbidden dependency directions should fail CI.

### 11.2 Important Principle

Grep-based architecture tests are useful. They are not the ideal final enforcement mechanism.

The preferred trajectory is:

- crate/module visibility makes the wrong thing hard or impossible,
- architecture tests catch regressions and migration leaks,
- social discipline is the last line of defense, not the first.

---

## 12. Large-Repo and IDE Architecture

tsz should aim for bounded residency and stable incremental work, not permanent full-program residency.

### 12.1 North Star

1. Binder produces a stable global semantic skeleton/index.
2. Semantic queries operate on stable identity, not direct AST reachability.
3. Files can be reparsed/rechecked without rebuilding the world.
4. ASTs and heavy arenas can eventually be evicted when derived facts are sufficient.
5. Incremental invalidation happens by semantic/API fingerprint, not by broad whole-project churn.

### 12.2 Consequences

This means we should prefer:

- stable declaration summaries,
- file-scoped semantic products,
- queryable project indexes,
- memory ownership that allows eviction,
- APIs designed around identity and facts rather than direct AST borrowing.

It also means we should not prematurely optimize for a scheduler over unstable identity.

---

## 13. Migration Priorities

When choosing major architectural work, prefer this order:

1. **Strengthen stable identity**  
   Make semantic identity earlier, cleaner, and more canonical.

2. **Make context explicit**  
   Remove ambient mutable context in favor of request-first APIs.

3. **Make speculation transactional**  
   Rollback semantics should be infrastructure, not call-site discipline.

4. **Unify semantic policy surfaces**  
   Especially property presence, object-literal freshness, call compatibility, and assignability.

5. **Make caches explicit and request-aware**  
   Replace blanket bypasses and recursive invalidation with designed cache keys and ownership.

6. **Bound memory and enable incrementalism**  
   Build toward global semantic skeletons and bounded residency.

7. **Normalize diagnostics after semantic stabilization**  
   Rendering policy should be centralized once semantic paths are stable.

Not every local bug fix must follow this order. Major architectural work should.

---

## 14. Anti-Goals

The following are explicitly not the north star:

1. Re-implementing solver logic locally in checker helpers.
2. Fixing correctness by adding more ambient mutable state.
3. Fixing cache unsoundness by permanently bypassing caches.
4. Fixing semantics in reporter formatting code.
5. Using recursive cache clearing as a normal compatibility mechanism.
6. Building large-repo scheduling before stable identity and bounded residency exist.
7. Treating arena layout or micro-optimizations as the primary architecture story.
8. Relying on architecture tests alone when module visibility could enforce the boundary directly.

Arena allocation, thin wrappers, and performance matter. They are means, not the north star.

---

## 15. Review Checklist

For any non-trivial PR, reviewers should ask:

1. Does this reduce the number of semantic paths or add another one?
2. Does this make context more explicit or more ambient?
3. Does this strengthen stable identity or recover it later in hot paths?
4. Does this centralize policy or scatter it?
5. Does this make speculative work more transactional or more caller-disciplined?
6. Does this improve cache ownership and audibility or add another bypass/clear path?
7. Does this move semantics downstream of requests and upstream of diagnostics?
8. Does this make the wrong architectural move harder in code, not just less polite?

If a change improves correctness but increases architectural duplication, it should usually be treated as a temporary patch and called out explicitly.

---

## 16. Success Metrics

We should measure progress with architecture-sensitive metrics, not only pass rate.

### 16.1 Structural Metrics

- Direct checker access to solver internals outside approved query boundaries trends to zero.
- Ambient contextual-state mutations trend to zero in checker hot paths.
- Speculative state rollback is infrastructure-backed, not open-coded.
- Recursive cache-clear sites trend toward zero.
- Long-lived caches have explicit ownership and rollback/invalidation rules.

### 16.2 Semantic Metrics

- The major compatibility families (`TS2322`, `TS2339`, `TS2345`, related property/freshness codes) stop showing both high missing and high extra counts.
- Object-literal, property-presence, and call-compatibility behavior converge on one semantic kernel.
- Diagnostic fingerprint drift drops after semantic stabilization.

### 16.3 Throughput Metrics

- Request-aware cache hit rates improve on contextual/object/property-heavy workloads.
- Optional-chain, union-property, and class-heavy workloads improve through summary/cache design rather than ad hoc shortcuts.
- Large-repo work reduces peak residency and recheck cost through stable semantic identity and bounded derived-state reuse.

---

## 17. Final Rule

When there is uncertainty, prefer the design that:

- has **one** semantic truth source,
- passes **explicit requests** instead of mutating ambient state,
- uses **stable identity** instead of reconstructing it,
- treats **speculation as transactional**,
- treats **caching as an architectural contract**,
- and keeps **diagnostics downstream of semantic facts**.

That is the direction tsz should keep converging toward.
