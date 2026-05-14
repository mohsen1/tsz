# 2026-05-14 Simple-Object Composite / Array / Tuple Admission

Follow-up to
[`2026-05-14-simple-object-primitive-literal-type-refs.md`](2026-05-14-simple-object-primitive-literal-type-refs.md).
That run left two guarded simple local-interface shortcut rejects on
monorepo-006: one `union_or_intersection` and one `array_or_tuple`.

## Change

The shortcut now admits composite and array-like annotations only when their
children are already accepted by the existing simple-fastpath predicate:

- `union` and `intersection` annotations whose members are simple;
- `array` annotations whose element type is simple;
- `tuple` annotations whose elements are simple.

The property type is still lowered by `get_type_from_type_node_in_type_literal`;
this change only widens the conservative admission gate.

## Reproducer

| Item | Value |
| --- | --- |
| Fixture | `scripts/bench/scale-cliff/fixtures/monorepo-006` |
| Fixture generation | `scripts/bench/scale-cliff/generate-fixtures.sh` |
| Build | `CARGO_TARGET_DIR=.target-simple-object-composite cargo build -p tsz-cli --bin tsz --release --features perf-tools` |
| Counter mode | `TSZ_PERF_COUNTERS=1` |
| Raw diagnostics | `docs/plan/perf-runs/raw/2026-05-14-simple-object-composite-array-tuple-monorepo-006-diag.json` |
| Raw counters | `docs/plan/perf-runs/raw/2026-05-14-simple-object-composite-array-tuple-monorepo-006-pc.json` |

Command:

```sh
TSZ_PERF_COUNTERS=1 .target-simple-object-composite/release/tsz \
  --noEmit \
  -p scripts/bench/scale-cliff/fixtures/monorepo-006/tsconfig.json \
  --extendedDiagnostics \
  --pretty false \
  --diagnostics-json /tmp/tsz-simple-object-composite-diag.json \
  --perf-counters-json /tmp/tsz-simple-object-composite-pc.json
```

The process exited with status 2 because the generated fixture still reports
diagnostics. Both JSON artifacts were written and parsed successfully.

## Counter Result

| Metric | Previous | After |
| --- | ---: | ---: |
| diagnostics | 10,198 | 10,198 |
| `checker.compute_type_of_symbol_interface_simple_object_fastpath_hits` | 24,760 | 24,760 |
| `compute_type_of_symbol_interface_simple_object_outcomes.success` | 24,760 | 24,760 |
| `compute_type_of_symbol_interface_simple_object_outcomes.reject_non_primitive_annotation` | 2 | 0 |
| `union_or_intersection` reject kind | 1 | 0 |
| `array_or_tuple` reject kind | 1 | 0 |
| `checker.with_parent_cache_constructed` | 11 | 5 |
| `delegate.misses` | 11 | 5 |

The run is attribution-mode and had noisy timing (`total=134.47s`,
`check=132.34s`), so it is not a timing claim.

## Decision

The guarded simple local-interface shortcut has no remaining
non-primitive-annotation residue on regenerated monorepo-006. The active
checker-side residue is back to declaration-file alias fallback:
`FlatArray` (2), `IteratorResult` (2), and `Partial` (1).
