# 2026-05-13 — `compute_type_of_symbol` Interface Simple-Object Outcomes (monorepo-006)

Follow-up to `2026-05-13-compute-type-of-symbol-interface-simple-local-object-hit-counter.md`.

Goal: classify why the simple local-interface object shortcut does or does not
apply on each interface-symbol `compute_type_of_symbol` call.

## Change

Add `compute_type_of_symbol_interface_simple_object_outcomes` as a named outcome
array in perf counters and JSON snapshot.

Buckets include:

- `success`
- structural reject gates (`out_of_arena_decl`, `cross_file_same_index`,
  `declaration_count`, `missing_interface_decl`)
- semantic reject gates (`type_parameters`, `heritage_extends`,
  `non_property_member`, `computed_name`, `unresolved_property_name`)

## Reproducer

| Item | Value |
| --- | --- |
| Raw artifact | `docs/plan/perf-runs/raw/2026-05-13-compute-type-of-symbol-interface-simple-object-outcomes-monorepo-006.json` |
| `tsz` build | `cargo build -p tsz-cli --bin tsz --release` |
| Fixture | `scripts/bench/scale-cliff/fixtures/monorepo-006` |
| Counter mode | `TSZ_PERF_COUNTERS=1` |
| Command | `.target/release/tsz --noEmit -p <fixture>/tsconfig.json --extendedDiagnostics --pretty false` |

## Result

From the run:

- diagnostics: `10,198`
- `compute_type_of_symbol.kind.interface`: `24,796`

Simple-object outcomes:

- `success`: `24,760` (`99.85%`)
- `reject_out_of_arena_decl`: `16`
- `reject_missing_interface_decl`: `7`
- `reject_declaration_count`: `1`
- `reject_heritage_extends`: `1`
- all other reject buckets: `0`

## Decision

1. Keep this outcome array; it narrows the non-hit residue to a tiny,
   concrete set of structural/heritage cases.
2. Target any next shortcut-expansion work at the active reject buckets only;
   avoid spending time on currently zero buckets until workloads show demand.

## Guarded rerun update

After guard narrowing, the active monorepo-006 rerun is tracked in
`2026-05-13-compute-type-of-symbol-interface-simple-local-object-guarded-rerun.md`.
The guarded branch reports:

- `success = 0`
- `checker.compute_type_of_symbol_interface_simple_object_fastpath_hits = 0`
- `reject_non_primitive_annotation = 24,760`
- `interface_simple_object_non_primitive_annotation_kinds.type_reference = 24,760`
- `interface_simple_object_type_reference_reject_outcomes.identifier_not_found_symbol = 24,760`

So the broad-run `99.85%` success ratio here is historical context only, not
the current guarded baseline.
