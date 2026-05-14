# 2026-05-14 Simple-Object Primitive And Literal Type References

Follow-up to the #6747 residue run. That run showed the guarded
simple local-interface shortcut was blocked first by primitive-looking
`number` type references, then by string-literal `tag` properties in the same
generated interfaces.

## Change

The shortcut now accepts:

- no-argument primitive intrinsic type references such as `number`, `string`,
  and `boolean`;
- literal and template-literal type annotations.

The actual property `TypeId` still comes from the existing
`get_type_from_type_node_in_type_literal` slow lowerer. This PR only widens
the conservative shortcut admission gate.

## Reproducer

| Item | Value |
| --- | --- |
| Fixture | `scripts/bench/scale-cliff/fixtures/monorepo-006` |
| Fixture generation | `scripts/bench/scale-cliff/generate-fixtures.sh` |
| Build | `CARGO_TARGET_DIR=/Users/mohsen/.cache/tsz-target cargo build -p tsz-cli --bin tsz --release --features perf-tools` |
| Counter mode | `TSZ_PERF_COUNTERS=1` |
| Raw diagnostics | `docs/plan/perf-runs/raw/2026-05-14-simple-object-primitive-literal-type-refs-monorepo-006-diag.json` |
| Raw counters | `docs/plan/perf-runs/raw/2026-05-14-simple-object-primitive-literal-type-refs-monorepo-006-pc.json` |

Command:

```sh
TSZ_PERF_COUNTERS=1 /Users/mohsen/.cache/tsz-target/release/tsz \
  --noEmit \
  -p scripts/bench/scale-cliff/fixtures/monorepo-006/tsconfig.json \
  --extendedDiagnostics \
  --pretty false \
  --diagnostics-json docs/plan/perf-runs/raw/2026-05-14-simple-object-primitive-literal-type-refs-monorepo-006-diag.json \
  --perf-counters-json docs/plan/perf-runs/raw/2026-05-14-simple-object-primitive-literal-type-refs-monorepo-006-pc.json
```

The process exited with status 2 because the generated fixture still reports
diagnostics. Both JSON artifacts were written and parsed successfully.

## Counter Result

| Metric | Before (#6747) | After |
| --- | ---: | ---: |
| diagnostics | 10,198 | 10,198 |
| `checker.compute_type_of_symbol_interface_simple_object_fastpath_hits` | 0 | 24,760 |
| `compute_type_of_symbol_interface_simple_object_outcomes.success` | 0 | 24,760 |
| `compute_type_of_symbol_interface_simple_object_outcomes.reject_non_primitive_annotation` | 24,762 | 2 |
| `type_reference` reject kind | 24,761 | 0 |
| `literal_or_template_literal` reject kind | 0 | 0 |
| `union_or_intersection` reject kind | 1 | 1 |
| `array_or_tuple` reject kind | 0 | 1 |
| type-reference reject residues | `number=24,761` | none |

The run is attribution-mode and had noisy timing (`total=92.36s`,
`check=90.33s`), so it is not a timing claim.

## Decision

The bulk guarded shortcut residue on monorepo-006 is now removed without a
general type resolver in the shortcut path. The remaining measured rejects are
two concrete non-primitive annotations: one union/intersection and one
array/tuple. Any follow-up should inspect those two source shapes before
admitting more annotation kinds.
