# 2026-05-14 Simple-Object Number/Literal Admission

Follow-up to
[`2026-05-14-simple-object-type-reference-residues.md`](2026-05-14-simple-object-type-reference-residues.md).

## Reproducer

| Item | Value |
| --- | --- |
| Fixture | `scripts/bench/scale-cliff/fixtures/monorepo-006` |
| Fixture generation | `scripts/bench/scale-cliff/generate-fixtures.sh` |
| Build | `CARGO_TARGET_DIR=/private/tmp/tsz-simple-number-target cargo build -p tsz-cli --bin tsz --release --features perf-tools` |
| Counter mode | `TSZ_PERF_COUNTERS=1` |
| Raw diagnostics | `docs/plan/perf-runs/raw/2026-05-14-simple-object-number-residue-diag.json` |
| Raw counters | `docs/plan/perf-runs/raw/2026-05-14-simple-object-number-residue-pc.json` |

Command:

```sh
TSZ_PERF_COUNTERS=1 /private/tmp/tsz-simple-number-target/release/tsz \
  --noEmit \
  -p scripts/bench/scale-cliff/fixtures/monorepo-006/tsconfig.json \
  --extendedDiagnostics \
  --pretty false \
  --diagnostics-json docs/plan/perf-runs/raw/2026-05-14-simple-object-number-residue-diag.json \
  --perf-counters-json docs/plan/perf-runs/raw/2026-05-14-simple-object-number-residue-pc.json
```

The process exited with status 2 because the generated fixture still reports
diagnostics. Both JSON artifacts were written and are usable for attribution.

## Result

| Metric | Before | After |
| --- | ---: | ---: |
| diagnostics | 10,198 | 10,198 |
| `checker.compute_type_of_symbol_interface_simple_object_fastpath_hits` | 0 | 24,760 |
| `compute_type_of_symbol_interface_simple_object_outcomes.success` | 0 | 24,760 |
| `compute_type_of_symbol_interface_simple_object_outcomes.reject_non_primitive_annotation` | 24,762 | 2 |
| `compute_type_of_symbol_interface_simple_object_non_primitive_annotation_kinds.type_reference` | 24,761 | 0 |
| `compute_type_of_symbol_interface_simple_object_non_primitive_annotation_kinds.literal_or_template_literal` | 0 | 0 |
| `compute_type_of_symbol_interface_simple_object_non_primitive_annotation_kinds.union_or_intersection` | 1 | 1 |
| `compute_type_of_symbol_interface_simple_object_non_primitive_annotation_kinds.array_or_tuple` | 0 | 1 |
| type-reference residue rows | `number=24,761` | none |

The remaining guarded shortcut rejects are now two concrete non-local cases:
one union/intersection annotation and one array/tuple annotation. The previously
dominant generated leaf interfaces are admitted through the simple-object path
after normalizing parser-produced primitive keyword type references and allowing
plain literal type annotations.

## Decision

The primitive-looking `number` residue was not a parser bug. The parser commonly
represents bare primitive keyword type names as `TYPE_REFERENCE` nodes. The
normal type-literal lowering already maps those references back to primitive
`TypeId`s, so the shortcut guard now mirrors that existing lowering for
resolver-free primitive names.

The generated leaf interfaces also contain literal tag annotations such as
`tag: "leaf-1"`. Admitting `LITERAL_TYPE` annotations is still local and
resolver-free, and it is required for the primitive `number` normalization to
turn into successful simple-object lowering on monorepo-006.

No timing claim is made from this run. It is attribution-mode and shared-runner
contention was visible in total/check time.
