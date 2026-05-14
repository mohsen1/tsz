# Claim: Name simple-object declaration/provenance residues

## Scope

Add an attribution-only perf-counter table for the remaining declaration /
provenance guards in the `compute_type_of_symbol` simple local-interface
shortcut.

The behavior is unchanged. The shortcut still falls back for
`reject_out_of_arena_decl` and `reject_missing_interface_decl`; this slice only
names the sparse rows before any future admission work.

## Result

The new JSON/text field is
`compute_type_of_symbol_interface_simple_object_declaration_provenance_residues`.
It records bounded `(outcome, symbol, declaration_count, count)` rows.

On regenerated monorepo-006:

| Metric | Count |
| --- | ---: |
| diagnostics | 10,198 |
| simple-object fast-path hits | 24,762 |
| simple-object success outcomes | 24,762 |
| declaration/provenance residue rows | 13 |
| `reject_out_of_arena_decl` rows | 6 |
| `reject_missing_interface_decl` rows | 7 |

Decision record:
[`docs/plan/perf-runs/2026-05-14-simple-object-provenance-residues.md`](../perf-runs/2026-05-14-simple-object-provenance-residues.md).

## Validation

- `cargo test -p tsz-common compute_type_of_symbol_interface_simple_object_declaration_provenance_residues_lock_field_shape -- --nocapture`
- `cargo test -p tsz-common snapshot_serializes_with_expected_top_level_keys -- --nocapture`
- `cargo check -p tsz-checker --lib`
- `cargo fmt --all --check`
- `git diff --check`
- `CARGO_INCREMENTAL=0 cargo build -p tsz-cli --bin tsz --release --features perf-tools`
- `TSZ_PERF_COUNTERS=1 .target/release/tsz --noEmit -p scripts/bench/scale-cliff/fixtures/monorepo-006/tsconfig.json --extendedDiagnostics --pretty false --diagnostics-json docs/plan/perf-runs/raw/2026-05-14-simple-object-provenance-residues-diag.json --perf-counters-json docs/plan/perf-runs/raw/2026-05-14-simple-object-provenance-residues-pc.json` (expected exit `2`)
