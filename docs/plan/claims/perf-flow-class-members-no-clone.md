# perf(checker): avoid cloning flow class members

- **Date**: 2026-05-05
- **Branch**: `perf/flow-class-members-no-clone`
- **PR**: #3028
- **Status**: ready
- **Workstream**: 5 (Stable Identity, Skeletons, And Large-Repo Residency)

## Intent

Avoid cloning a class member vector while building class declaration control
flow. The scan only needs one `NodeIndex` at a time; copying individual indices
keeps the arena borrow short without allocating a temporary vector.

## Planned Scope

- `crates/tsz-checker/src/flow/flow_graph_builder/expressions.rs`
- `docs/plan/claims/perf-flow-class-members-no-clone.md`

## Verification

- `cargo test -p tsz-checker --lib test_flow_graph_class -- --nocapture`
  (pass: 6 passed)
- `scripts/bench/perf-hotspots.sh --quick`
  (pass, artifact: `artifacts/perf/hotspots-20260505-073243.json`; tsz beat
  tsgo on all five quick fixtures: 100 classes 2.39x, Constraint conflicts
  N=30 1.73x, 50 generic functions 1.29x, Shallow optional-chain N=50 1.20x,
  DeepPartial optional-chain N=50 1.18x)
- `cargo fmt --check` (pass)
- `git diff --check` (pass)
