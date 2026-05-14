# 2026-05-14 Simple-Object Residual Annotations

Follow-up to
[`2026-05-14-simple-object-primitive-literal-type-refs.md`](2026-05-14-simple-object-primitive-literal-type-refs.md).

## Change

The simple local-interface shortcut now accepts recursively simple
union/intersection, array, and tuple annotations when every child annotation is
already admitted by the same shortcut guard. This keeps arbitrary type
references and other resolver-dependent annotations on fallback.

The property `TypeId` still comes from the existing
`get_type_from_type_node_in_type_literal` lowerer. This PR only widens the
conservative admission gate for resolver-free composite shapes.

## Reproducer

| Item | Value |
| --- | --- |
| Fixture | `scripts/bench/scale-cliff/fixtures/monorepo-006` |
| Fixture generation | already present from `scripts/bench/scale-cliff/generate-fixtures.sh` |
| Build | `CARGO_TARGET_DIR=/private/tmp/tsz-simple-number-target cargo build -p tsz-cli --bin tsz --release --features perf-tools` |
| Counter mode | `TSZ_PERF_COUNTERS=1` |
| Raw diagnostics | `docs/plan/perf-runs/raw/2026-05-14-simple-object-residual-annotations-diag.json` |
| Raw counters | `docs/plan/perf-runs/raw/2026-05-14-simple-object-residual-annotations-pc.json` |

Command:

```sh
TSZ_PERF_COUNTERS=1 /private/tmp/tsz-simple-number-target/release/tsz \
  --noEmit \
  -p scripts/bench/scale-cliff/fixtures/monorepo-006/tsconfig.json \
  --extendedDiagnostics \
  --pretty false \
  --diagnostics-json docs/plan/perf-runs/raw/2026-05-14-simple-object-residual-annotations-diag.json \
  --perf-counters-json docs/plan/perf-runs/raw/2026-05-14-simple-object-residual-annotations-pc.json
```

The process exited with status 2 because the generated fixture still reports
diagnostics. Both JSON artifacts were written and parsed successfully.

## Counter Result

| Metric | Before | After |
| --- | ---: | ---: |
| diagnostics | 10,198 | 10,198 |
| `checker.compute_type_of_symbol_interface_simple_object_fastpath_hits` | 24,760 | 24,762 |
| `compute_type_of_symbol_interface_simple_object_outcomes.success` | 24,760 | 24,762 |
| `compute_type_of_symbol_interface_simple_object_outcomes.reject_non_primitive_annotation` | 2 | 0 |
| `union_or_intersection` reject kind | 1 | 0 |
| `array_or_tuple` reject kind | 1 | 0 |

The run is attribution-mode (`total=86.68s`, `check=84.60s`), so it is not a
timing claim.

## Decision

The measured simple local-interface shortcut annotation residue on
monorepo-006 is now exhausted without adding a general resolver path to the
shortcut. The remaining shortcut rejects are declaration/provenance guards:
`reject_out_of_arena_decl=6` and `reject_missing_interface_decl=7`.
