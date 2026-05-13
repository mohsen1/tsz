# 2026-05-13 — `compute_type_of_symbol` Simple-Local-Object Guarded Rerun (monorepo-006)

Follow-up to:

- `2026-05-13-compute-type-of-symbol-interface-simple-local-object-fastpath.md`
- `2026-05-13-compute-type-of-symbol-interface-simple-local-object-hit-counter.md`
- `2026-05-13-compute-type-of-symbol-interface-simple-object-outcomes.md`

Goal: refresh the monorepo-006 attribution baseline after the guard narrowing
that rejects empty interfaces and non-primitive member annotations.

## Reproducer

| Item | Value |
| --- | --- |
| Raw artifact | `docs/plan/perf-runs/raw/2026-05-13-compute-type-of-symbol-interface-simple-local-object-guarded-monorepo-006.json` |
| `tsz` build | `CARGO_TARGET_DIR=/Users/mohsen/.cache/tsz-target cargo build -p tsz-cli --bin tsz --release` |
| Fixture | `scripts/bench/scale-cliff/fixtures/monorepo-006` |
| Counter mode | `TSZ_PERF_COUNTERS=1` |
| Command | `/Users/mohsen/.cache/tsz-target/release/tsz --noEmit -p <fixture>/tsconfig.json --extendedDiagnostics --pretty false` |

## Result

Primary attribution run (`timings` in raw JSON):

- diagnostics: `10,198`
- total/check: `102.01s / 98.43s`
- `compute_type_of_symbol.total_calls`: `26,377`
- `compute_type_of_symbol.kind.interface`: `24,796`
- `checker.compute_type_of_symbol_interface_simple_object_fastpath_hits`: `0`
- `compute_type_of_symbol_interface_simple_object_outcomes.success`: `0`

Stable counter signal:

- active simple-object reject residue remains:
  - `reject_out_of_arena_decl=16`
  - `reject_missing_interface_decl=7`
  - `reject_declaration_count=1`
  - `reject_heritage_extends=1`
  - `reject_non_primitive_annotation=24,760`
- non-primitive annotation split (new):
  - `type_reference=24,760`
  - all other annotation-kind buckets `=0`

Interpretation:

- the guarded shortcut is currently inactive on monorepo-006;
  root interface demand is still handled by the older interface fast-path
  matrix (`skip_all_three=24,767`) plus full-path/interface-lowering fallback.

## Decision

1. Treat the earlier broad-shortcut hit/success ratios as historical only.
2. Use this guarded rerun as the active baseline for future interface-demand work.
3. Next shortcut work should be either:
   - a conformance-proven `type_reference` guard relaxation that restores
     meaningful `success`, or
   - deletion/simplification of dead shortcut branches if they remain inactive.
