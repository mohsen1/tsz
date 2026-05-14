# 2026-05-14 Simple-Object Declaration Provenance Residues

Follow-up to
[`2026-05-14-simple-object-residual-annotations.md`](2026-05-14-simple-object-residual-annotations.md).

## Change

Add a bounded counter table for declaration/provenance guard rejects in the
simple local-interface object shortcut:

`compute_type_of_symbol_interface_simple_object_declaration_provenance_residues`

Rows are `(outcome, symbol, declaration_count, count)`. The field is
attribution-only and does not change shortcut admission.

## Reproducer

| Item | Value |
| --- | --- |
| Fixture | `scripts/bench/scale-cliff/fixtures/monorepo-006` |
| Build | `CARGO_INCREMENTAL=0 cargo build -p tsz-cli --bin tsz --release --features perf-tools` |
| Counter mode | `TSZ_PERF_COUNTERS=1` |
| Raw diagnostics | `docs/plan/perf-runs/raw/2026-05-14-simple-object-provenance-residues-diag.json` |
| Raw counters | `docs/plan/perf-runs/raw/2026-05-14-simple-object-provenance-residues-pc.json` |

Command:

```sh
TSZ_PERF_COUNTERS=1 .target/release/tsz \
  --noEmit \
  -p scripts/bench/scale-cliff/fixtures/monorepo-006/tsconfig.json \
  --extendedDiagnostics \
  --pretty false \
  --diagnostics-json docs/plan/perf-runs/raw/2026-05-14-simple-object-provenance-residues-diag.json \
  --perf-counters-json docs/plan/perf-runs/raw/2026-05-14-simple-object-provenance-residues-pc.json
```

The process exited with status 2 because the generated fixture still reports
diagnostics. Both JSON artifacts were written and parsed successfully.

## Counter Result

| Metric | Count |
| --- | ---: |
| diagnostics | 10,198 |
| files / lib files | 5337 / 87 |
| `checker.compute_type_of_symbol_interface_simple_object_fastpath_hits` | 24,762 |
| `compute_type_of_symbol_interface_simple_object_outcomes.success` | 24,762 |
| `reject_out_of_arena_decl` | 6 |
| `reject_missing_interface_decl` | 7 |
| `reject_non_primitive_annotation` | 0 |
| `delegate.misses` | 2 |
| `checker.with_parent_cache_constructed` | 2 |

`reject_missing_interface_decl` rows:

| Symbol | Declarations | Count |
| --- | ---: | ---: |
| `Iterable` | 1 | 1 |
| `IteratorReturnResult` | 1 | 1 |
| `IteratorYieldResult` | 1 | 1 |
| `PropertyDescriptor` | 1 | 1 |
| `PropertyDescriptorMap` | 1 | 1 |
| `RegExpIndicesArray` | 1 | 1 |
| `RegExpStringIterator` | 1 | 1 |

`reject_out_of_arena_decl` rows:

| Symbol | Declarations | Count |
| --- | ---: | ---: |
| `ArrayIterator` | 1 | 1 |
| `CollatorOptions` | 1 | 1 |
| `DateTimeFormatOptions` | 3 | 1 |
| `IteratorObject` | 3 | 1 |
| `NumberFormatOptions` | 3 | 1 |
| `StringIterator` | 1 | 1 |

The run is attribution-mode for counters (`total=80.82s`, `check=78.48s`), so
it is not a timing claim.

## Decision

The annotation-kind residue is exhausted, and the remaining simple-object
shortcut rejects are concrete declaration/provenance rows. The next behavior
slice should avoid a broad guard relaxation: prove one row family at a time,
starting with whether the missing-interface rows can be resolved from existing
local/global lib metadata without constructing a child checker.
