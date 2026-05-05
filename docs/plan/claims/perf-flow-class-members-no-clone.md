# perf(checker): avoid cloning flow class members

- **Date**: 2026-05-05
- **Branch**: `perf/flow-class-members-no-clone`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 5 (Stable Identity, Skeletons, And Large-Repo Residency)

## Intent

Avoid cloning a class member vector while building class declaration control
flow. The scan only needs one `NodeIndex` at a time; copying individual indices
keeps the arena borrow short without allocating a temporary vector.

## Planned Scope

- `crates/tsz-checker/src/flow/flow_graph_builder/expressions.rs`
- `docs/plan/claims/perf-flow-class-members-no-clone.md`

## Verification

- TBD
