# perf(solver): extend type_contains_infer terminal-kind fast path

- **Date**: 2026-05-01
- **Branch**: `perf/solver-type-contains-infer-fast-path`
- **PR**: TBD
- **Status**: ready
- **Workstream**: §18 (Performance Targets — hot paths avoid per-op heap allocation)

## Intent

`type_contains_infer` is called multiple times per `evaluate_conditional`
invocation (twice in `try_application_infer_match` + main flow + the
identity short-circuit at line 393 + the deferral check at line 363).
The pre-existing fast path only short-circuited on intrinsic types and
direct `Infer`; for every other type kind — including the very common
`Lazy(DefId)`, `TypeQuery`, and other terminal kinds — the function
allocated a fresh `FxHashSet<TypeId>` and entered the recursive
walker just to return `false` from the leaf arm.

The walker's leaf arm already enumerated the terminal-kind set:

```rust
TypeData::Intrinsic(_) | TypeData::Literal(_) | TypeData::Lazy(_)
    | TypeData::Recursive(_) | TypeData::BoundParameter(_)
    | TypeData::TypeQuery(_) | TypeData::UniqueSymbol(_)
    | TypeData::ThisType | TypeData::ModuleNamespace(_)
    | TypeData::UnresolvedTypeName(_) | TypeData::Error => false,
```

Promoting that set to the entry-point fast path skips both the
`FxHashSet::default()` allocation and the unused `visited.insert`
bookkeeping for the common case where `extends_type` is a generic
interface reference (`Lazy(DefId)`) or a primitive.

The single `lookup` at the entry point is reused via a new
`type_contains_infer_inner_with_key` so the recursive walker doesn't
re-fetch the same `TypeData` immediately.

## Files Touched

- `crates/tsz-solver/src/evaluation/evaluate_rules/infer_pattern.rs`
  (~50 LOC: extended fast path + lookup-reuse helper).

## Verification

- `cargo nextest run -p tsz-checker -p tsz-solver --lib` → **8662/8662 pass**.
- The fast-path extension is *purely* a performance refactor: each
  newly short-circuited type kind matches the recursive walker's leaf
  return value (`false`), so behaviour is unchanged.
- The lookup-reuse helper is a refactor of the existing recursive path —
  same semantics, one fewer interner lookup per call.

## Rationale

`type_contains_infer` is an inner-loop predicate inside the conditional
type evaluator. Avoiding its `FxHashSet` allocation in the common case
is a textbook example of CLAUDE.md §18: "Hot paths avoid per-op heap
allocation where practical."
