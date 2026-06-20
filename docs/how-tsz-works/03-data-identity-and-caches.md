# Data, Identity, And Caches

The compiler is only as stable as its identities. `tsz` intentionally prefers
small canonical handles and arenas over ad hoc object identity.

## Canonical Handles

Common identity handles include:

| Handle | Owner | Meaning |
| --- | --- | --- |
| `Atom` / `AstAtom` | `crates/tsz-common/src/interner.rs` and consumers | Interned text identity. |
| `NodeIndex` | `crates/tsz-parser` | AST arena coordinate, not semantic identity. |
| `SymbolId` | `crates/tsz-binder` | Binder-owned symbol identity. |
| `ScopeId` | `crates/tsz-binder` | Binder-owned scope identity. |
| `FlowNodeId` | `crates/tsz-binder` / checker flow | Control-flow graph node identity. |
| `DefId` | binder/checker/solver boundary | Stable semantic reference for lazy definitions. |
| `TypeId` | `crates/tsz-solver` | Interned semantic type identity. |

The architecture rule is that `NodeIndex` is not enough for cross-file semantic
reuse. If a semantic fact must be stable across files, sessions, caches, or
reordered declarations, it needs a semantic identity such as `SymbolId`,
`DefId`, or `TypeId`.

## Arenas And Interning

The parser writes syntax into arenas so later layers can use compact indices.
The solver interns semantic types so repeated type structures have O(1)
identity comparisons where possible. The common crate exposes shared span,
position, id, interner, source-map, and option utilities through
`crates/tsz-common/src/lib.rs`.

Interning helps performance only when cache keys are semantically valid. A cache
that keys on a transient AST position can look fast while producing schedule- or
file-order-dependent behavior.

## Lazy Semantic References

Semantic references are commonly represented as lazy `DefId`-backed work. The
checker stabilizes the `DefId`; `TypeEnvironment` and solver-backed evaluation
resolve the definition into a `TypeId` when needed.

That split supports:

- cross-file reuse without eagerly materializing the world;
- avoiding repeated evaluation of generic and mapped aliases;
- preserving order-independent diagnostics;
- keeping checker traversal separate from solver semantics.

## Cache Ownership

Cache ownership follows the same layer rules:

- Parser caches must not depend on type semantics.
- Binder caches can remember scopes, symbols, declaration summaries, and module
  surfaces.
- Checker caches can remember request construction, diagnostic bookkeeping,
  source-file state, and solver-backed answers keyed by stable identities.
- Solver caches can remember semantic facts, relations, instantiations,
  evaluation results, narrowing facts, and type construction results.
- Emitter caches can remember output planning and already checked summaries, but
  must not rediscover semantic truth.

Important checker cache modules live under `crates/tsz-checker/src/context/` and
`crates/tsz-checker/src/state/`. Important solver cache modules live under
`crates/tsz-solver/src/caches/`, `crates/tsz-solver/src/intern/`, and the
relation/evaluation/instantiation modules that use them.

## Cache Failure Smells

When debugging parity or performance problems, suspect identity/caching when:

- `T` is reported as not assignable to `T`.
- cache-enabled and cache-disabled runs disagree;
- declaration order changes diagnostics;
- the same project result changes under parallelism;
- a fast path checks user-chosen names or printed type strings;
- a cache key includes a source path or fixture name as semantic input.

The fix should name the repeated semantic operation, the stable key, the
invalidation/residency expectation, and the owner layer.
