# fix(solver): make TS2739 missing-property sort total

- **Date**: 2026-05-02
- **Branch**: `fix/ts2739-total-order-comparator`
- **PR**: #2234
- **Status**: ready
- **Workstream**: 5 (large-repo fixture stability)

## Intent

Fix the TS2739/TS2741 missing-property ordering comparator so distinct
properties never compare equal when their declaration-order metadata ties.
The large-repo RSS sample exposed a Rust sort panic in this path after #2146
started ordering missing properties by declaration order.

## Planned Scope

- `crates/tsz-solver/src/relations/subtype/explain.rs`
- Focused regression coverage if there is a compact existing test seam
- `docs/plan/claims/fix-ts2739-missing-property-sort-total-order.md`

## Verification

- `cargo fmt --check` (pass)
- `cargo check -p tsz-solver` (pass)
- `cargo test -p tsz-checker ts2739_lists_missing_properties_in_declaration_order` (pass; 2/2)
- `CARGO_TARGET_DIR=.target-bench cargo build --profile dist -p tsz-cli --bin tsz` (pass)
- Guarded large-repo retry:
  `RUST_BACKTRACE=1 scripts/safe-run.sh --limit 75% --interval 2 --verbose -- .target-bench/dist/tsz --extendedDiagnostics --noEmit -p ~/code/large-ts-repo/tsconfig.flat.bench.json`;
  manual stop after stable sample window, exit 143, no sort panic observed, peak sampled physical footprint ~11.47 GB / 12.29 GB guard.
