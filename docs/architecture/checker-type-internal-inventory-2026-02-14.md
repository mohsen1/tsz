# Checker Type-Internal Inventory (2026-02-14)

Status: Phase 0 inventory snapshot  
Date: 2026-02-14  
Goal: Identify checker sites that directly inspect interned type internals.

## Method

Searches used:

```bash
rg -n "types\\.lookup\\(|db\\.lookup\\(" crates/tsz-checker/src --glob '!**/tests/**' -S
rg -n "TypeKey::" crates/tsz-checker/src --glob '!**/tests/**' -S
```

## High-Priority Migration Targets

These are active checker paths that branch on low-level type internals and should move behind solver query helpers first.

1. `crates/tsz-checker/src/state_type_resolution.rs`
   - Direct `TypeKey` matching on `Application`, `Lazy`, `TypeParameter`.
   - Deep in type resolution and call/result shaping.
   - Risk: high (relation/evaluation behavior drift).
2. `crates/tsz-checker/src/iterators.rs`
   - Direct `TypeKey` matching on `Object`, `Union`, `Function`.
   - Iterator protocol checks in checker.
   - Risk: high (false positives in iterable/async iterable cases).
3. `crates/tsz-checker/src/state_type_environment.rs`
   - `lookup()` + `TypeKey` matching on `Object` and `Lazy`.
   - DefId/lazy resolution bridging.
   - Risk: medium-high (DefId environment guarantees).
4. `crates/tsz-checker/src/generators.rs`
   - `lookup()` + `TypeKey::Object` checks for async generator shaping.
   - Risk: medium.

## Medium-Priority Migration Targets

1. `crates/tsz-checker/src/query_boundaries/iterable_checker.rs`
   - `db.lookup(type_id)` with `TypeKey::Literal` string check.
2. `crates/tsz-checker/src/call_checker.rs`
   - Debug output reads low-level `lookup()` result (non-semantic).
3. `crates/tsz-checker/src/context.rs`
   - Creates `TypeKey::Lazy(DefId)` references; mostly construction, not relation logic.

## Quick-Win Candidate Order

1. Move iterator shape predicates to `solver::queries` (`is_object_like`, `is_union_like`, `has_next_signature`).
2. Add a single solver helper for "application base lazy def id" to remove repeated checker matches.
3. Add `TypeEnvironment` helper query for resolved lazy-def extraction.

## Owner Proposal

1. `state_type_resolution.rs` -> Checker/solver boundary owner.
2. `iterators.rs` + `generators.rs` -> Flow/narrowing owner.
3. `state_type_environment.rs` -> DefId environment owner.
