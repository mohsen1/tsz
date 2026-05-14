# Claim: Simple-object type-reference reject residues

- **Date**: 2026-05-14
- **Branch**: `codex/perf-simple-object-type-ref-residues-20260514`
- **PR**: TBD
- **Status**: ready
- **Workstream**: Performance plan simple local-interface shortcut attribution

## Claim

The next simple local-interface shortcut slice should stay instrumentation-only.
Before relaxing symbol-resolution guards or deleting the inactive guarded path,
the `identifier_not_found_symbol = 24,760` bucket needs name-level attribution.

This PR adds a bounded
`compute_type_of_symbol_interface_simple_object_type_reference_reject_residues`
table to perf-counter JSON/text output. The table records `(name, outcome,
count)` rows for rejected type-reference annotations in perf-counter mode.

## Scope

- Do not admit additional annotation kinds into the simple-object shortcut.
- Do not change type resolution, diagnostics, or emitted output.
- Cap distinct residue rows and aggregate overflow to avoid unbounded memory
  growth in attribution runs.
- Preserve the existing stable named-array counters for outcome totals.

## Evidence

- `crates/tsz-common/src/perf_counters.rs`
  - adds the bounded residue table, snapshot field, text dump, and field-shape
    lock test.
- `crates/tsz-checker/src/state/type_analysis/computed/simple_local_interface.rs`
  - records identifier and qualified-name type-reference residues only when
    perf counters are enabled.

## Validation

- `cargo test -p tsz-common compute_type_of_symbol_interface_simple_object -- --nocapture`
- `cargo test -p tsz-common snapshot_serializes_with_expected_top_level_keys -- --nocapture`
- `cargo check -p tsz-checker --lib`
- `cargo fmt --all --check`
- `git diff --check`
