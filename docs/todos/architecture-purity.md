# Architecture Purity Worklist

Last updated: 2026-02-22
Owner: Compiler contributors

## Why this exists

`docs/architecture.html` is the target model for TSZ, but a few implementation paths are still transitional.  
This file tracks concrete work needed to close the remaining "current state vs target state" gaps.

## In-progress purity gaps

- [ ] Move checker-side `+` chain semantic fast path into solver query/evaluator.
  - Current location: `crates/tsz-checker/src/types/type_computation_binary.rs`
  - Target: solver-owned binary op query with checker as orchestration only.

- [ ] Move remaining checker-owned condition narrowing policy behind solver-owned narrowing entrypoints.
  - Current location: `crates/tsz-checker/src/flow/control_flow_condition_narrowing.rs`
  - Target: checker requests narrowing; solver owns narrowing algorithm details.

- [ ] Reduce direct checker usage of broad `tsz_solver::type_queries::*` helpers where a dedicated query boundary should exist.
  - Current location: multiple checker modules outside `query_boundaries/`
  - Target: boundary helper per concern (`assignability`, `property_access`, `flow_analysis`, etc.).

- [ ] Harden TS2322-family gateway enforcement for all legacy paths.
  - Current state: new work uses `query_boundaries::assignability`; some legacy call sites remain.
  - Target: all TS2322/TS2345/TS2416 relation+reason flow through one boundary gateway.

## Type-universe clarity

- [ ] Document and audit local-vs-global `TypeId` semantics where ephemeral IDs are used.
  - Current references: `crates/tsz-solver/src/types.rs` (`LOCAL_MASK`, `is_local`, `is_global`)
  - Target: explicit invariants and "no local ID escapes" checks for diagnostics/cache/public boundaries.

## Docs fidelity tasks

- [ ] Keep architecture doc metrics synchronized with repository reality.
  - Conformance metric source: `README.md`
  - LOC figures source: generated from `crates/*/src/*.rs`

- [ ] Add a lightweight CI/doc check that fails when architecture-page metrics drift beyond a tolerance window.

## Exit criteria

- Checker does not contain semantic type algorithm implementations that should be solver-owned.
- TS2322/TS2345/TS2416 all route through one assignability gateway.
- Architecture page statements are either exact current behavior or explicitly labeled as target/in-progress.
