# 2026-05-14 Simple-Object Nonprimitive Residues

Follow-up to #6753. That PR removed the bulk simple local-interface shortcut
residue by admitting primitive intrinsic type references and literal/template
literal annotations. The only remaining live rows on monorepo-006 were
`union_or_intersection=1` and `array_or_tuple=1`.

## Change

This run adds bounded attribution for every
`reject_non_primitive_annotation` row in the simple local-interface shortcut.
The new table records `(kind, interface, property, count)`.

Checker behavior is unchanged. This run does not admit unions,
intersections, arrays, tuples, aliases, or any other non-primitive annotation.

## Reproducer

| Item | Value |
| --- | --- |
| Fixture | `scripts/bench/scale-cliff/fixtures/monorepo-006` |
| Fixture generation | `scripts/bench/scale-cliff/generate-fixtures.sh` |
| Build | `CARGO_TARGET_DIR=/Users/mohsen/.cache/tsz-target cargo build -p tsz-cli --bin tsz --release --features perf-tools` |
| Counter mode | `TSZ_PERF_COUNTERS=1` |
| Raw diagnostics | `docs/plan/perf-runs/raw/2026-05-14-simple-object-nonprimitive-residues-monorepo-006-diag.json` |
| Raw counters | `docs/plan/perf-runs/raw/2026-05-14-simple-object-nonprimitive-residues-monorepo-006-pc.json` |

Command:

```sh
TSZ_PERF_COUNTERS=1 /Users/mohsen/.cache/tsz-target/release/tsz \
  --noEmit \
  -p scripts/bench/scale-cliff/fixtures/monorepo-006/tsconfig.json \
  --extendedDiagnostics \
  --pretty false \
  --diagnostics-json docs/plan/perf-runs/raw/2026-05-14-simple-object-nonprimitive-residues-monorepo-006-diag.json \
  --perf-counters-json docs/plan/perf-runs/raw/2026-05-14-simple-object-nonprimitive-residues-monorepo-006-pc.json
```

The process exited with status 2 because the generated fixture still reports
diagnostics. Both JSON artifacts were written and parsed successfully.

## Counter Result

| Metric | Count |
| --- | ---: |
| diagnostics | 10,198 |
| `checker.compute_type_of_symbol_interface_simple_object_fastpath_hits` | 24,760 |
| `compute_type_of_symbol_interface_simple_object_outcomes.success` | 24,760 |
| `compute_type_of_symbol_interface_simple_object_outcomes.reject_non_primitive_annotation` | 2 |
| `union_or_intersection` reject kind | 1 |
| `array_or_tuple` reject kind | 1 |
| type-reference reject residues | 0 |

Residue table:

| Kind | Interface | Property | Count |
| --- | --- | --- | ---: |
| `array_or_tuple` | `WeekInfo` | `weekend` | 1 |
| `union_or_intersection` | `TextInfo` | `direction` | 1 |

Timing in this attribution run is noisy (`total=111.01s`, `check=108.58s`),
so this is not a timing claim.

## Decision

The remaining simple-object shortcut residue was from actual lib interfaces:
`TextInfo.direction` and `WeekInfo.weekend`. The subsequent residual annotation
admission consumes both named rows. This PR only records the residue.
