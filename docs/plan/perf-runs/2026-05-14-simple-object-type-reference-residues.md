# 2026-05-14 Simple-Object Type-Reference Residues

Follow-up to #6734, which added the bounded
`compute_type_of_symbol_interface_simple_object_type_reference_reject_residues`
table.

## Reproducer

| Item | Value |
| --- | --- |
| Code commit | `95fafc52ff` |
| Fixture | `scripts/bench/scale-cliff/fixtures/monorepo-006` |
| Fixture generation | `scripts/bench/scale-cliff/generate-fixtures.sh` |
| Build | `CARGO_TARGET_DIR=/Users/mohsen/.cache/tsz-target cargo build -p tsz-cli --bin tsz --release --features perf-tools` |
| Counter mode | `TSZ_PERF_COUNTERS=1` |
| Raw diagnostics | `docs/plan/perf-runs/raw/2026-05-14-simple-object-type-ref-residues-monorepo-006-diag.json` |
| Raw counters | `docs/plan/perf-runs/raw/2026-05-14-simple-object-type-ref-residues-monorepo-006-pc.json` |

Command:

```sh
TSZ_PERF_COUNTERS=1 /Users/mohsen/.cache/tsz-target/release/tsz \
  --noEmit \
  -p scripts/bench/scale-cliff/fixtures/monorepo-006/tsconfig.json \
  --extendedDiagnostics \
  --pretty false \
  --diagnostics-json docs/plan/perf-runs/raw/2026-05-14-simple-object-type-ref-residues-monorepo-006-diag.json \
  --perf-counters-json docs/plan/perf-runs/raw/2026-05-14-simple-object-type-ref-residues-monorepo-006-pc.json
```

The process exited with status 2 because the generated fixture still reports
diagnostics. Both JSON artifacts were written and are usable for attribution.

## Result

| Metric | Value |
| --- | ---: |
| files | 5,337 |
| diagnostics | 10,198 |
| total time | 89.58 s |
| check time | 87.51 s |
| `checker.compute_type_of_symbol_interface_simple_object_fastpath_hits` | 0 |
| `compute_type_of_symbol_interface_simple_object_outcomes.success` | 0 |
| `compute_type_of_symbol_interface_simple_object_outcomes.reject_non_primitive_annotation` | 24,762 |
| `compute_type_of_symbol_interface_simple_object_non_primitive_annotation_kinds.type_reference` | 24,761 |
| `compute_type_of_symbol_interface_simple_object_non_primitive_annotation_kinds.union_or_intersection` | 1 |
| `compute_type_of_symbol_interface_simple_object_type_reference_reject_outcomes.identifier_not_found_symbol` | 24,761 |

Residue table:

| name | outcome | count |
| --- | --- | ---: |
| `number` | `identifier_not_found_symbol` | 24,761 |

## Decision

The live guarded shortcut residue is not a broad unresolved-symbol set. It is
one primitive-looking name, `number`, reported through the type-reference
reject path.

The next behavior PR should therefore avoid a general resolver rewrite. The
narrow next target is a conformance-proven investigation of why primitive
`number` reaches the shortcut as a type reference in this path, followed by
either:

1. normalize/admit the primitive `number` type-reference case in the simple
   local-interface shortcut, or
2. fix the parser/classification boundary if `number` should have been a
   `NumberKeyword` node before the shortcut sees it.

No timing claim is made from this run. It is attribution-mode and the fixture
still reports diagnostics.
