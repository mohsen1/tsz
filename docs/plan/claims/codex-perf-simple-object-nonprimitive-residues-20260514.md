# Claim: Simple-object nonprimitive residue attribution

- **Date**: 2026-05-14
- **Branch**: `codex/perf-simple-object-nonprimitive-residues-20260514`
- **PR**: #6766
- **Status**: ready
- **Workstream**: Performance plan simple local-interface shortcut attribution

## Claim

After #6753, the guarded simple local-interface shortcut has only two live
non-primitive rejects on regenerated monorepo-006:
`union_or_intersection=1` and `array_or_tuple=1`.

This PR adds bounded attribution for all
`reject_non_primitive_annotation` rows so behavior PRs can inspect exact
interface/property shapes before admitting more annotation kinds.

## Scope

- Add a bounded
  `compute_type_of_symbol_interface_simple_object_non_primitive_annotation_residues`
  perf-counter table.
- Record `(kind, interface, property, count)` for simple-object
  non-primitive annotation rejects.
- Keep checker behavior unchanged: this PR does not admit unions,
  intersections, arrays, tuples, aliases, or other non-primitive annotations.

## Evidence

- `crates/tsz-common/src/perf_counters.rs` serializes and snapshots the new
  bounded residue table.
- `crates/tsz-checker/src/state/type_analysis/computed/simple_local_interface.rs`
  wires the existing reject site to the new counter only after the fastpath has
  already decided to reject.
- `docs/plan/perf-runs/2026-05-14-simple-object-nonprimitive-residues.md`
  records the monorepo-006 attribution result:
  `TextInfo.direction` for the union/intersection row and `WeekInfo.weekend`
  for the array/tuple row.

## Validation

- `cargo test -p tsz-common compute_type_of_symbol_interface_simple_object -- --nocapture`
- `cargo test -p tsz-common snapshot_serializes_with_expected_top_level_keys -- --nocapture`
- `cargo check -p tsz-checker --lib`
- `cargo fmt --all --check`
- `git diff --check`
- `CARGO_TARGET_DIR=/Users/mohsen/.cache/tsz-target cargo build -p tsz-cli --bin tsz --release --features perf-tools`
- `TSZ_PERF_COUNTERS=1 /Users/mohsen/.cache/tsz-target/release/tsz --noEmit -p scripts/bench/scale-cliff/fixtures/monorepo-006/tsconfig.json --extendedDiagnostics --pretty false --diagnostics-json docs/plan/perf-runs/raw/2026-05-14-simple-object-nonprimitive-residues-monorepo-006-diag.json --perf-counters-json docs/plan/perf-runs/raw/2026-05-14-simple-object-nonprimitive-residues-monorepo-006-pc.json`
