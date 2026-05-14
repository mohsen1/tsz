# 2026-05-14 Simple-Object Missing-Interface Lib Resolution

Follow-up to
[`2026-05-14-simple-object-provenance-residues.md`](2026-05-14-simple-object-provenance-residues.md).

## Change

The simple local-interface object shortcut now routes the named
`reject_missing_interface_decl` residue family through existing lib metadata
before recording the reject. The admission is limited to:

- `Iterable`
- `IteratorReturnResult`
- `IteratorYieldResult`
- `PropertyDescriptor`
- `PropertyDescriptorMap`
- `RegExpIndicesArray`
- `RegExpStringIterator`

The path uses `resolve_lib_type_by_name`, with the existing parameter-aware lib
resolver as a fallback for generic lib interfaces. It does not manually lower
declaration arenas and leaves `reject_out_of_arena_decl` rows unchanged.

## Reproducer

| Item | Value |
| --- | --- |
| Base | `origin/main` at `1187c59113` |
| Fixture | `/private/tmp/tsz-bench-fixtures/monorepo-006` |
| Build | `CARGO_TARGET_DIR=/private/tmp/tsz-simple-missing-target CARGO_INCREMENTAL=0 cargo build -p tsz-cli --bin tsz --release --features perf-tools` |
| Counter mode | `TSZ_PERF_COUNTERS=1` |
| Raw diagnostics | `docs/plan/perf-runs/raw/2026-05-14-simple-object-missing-interface-lib-diag.json` |
| Raw counters | `docs/plan/perf-runs/raw/2026-05-14-simple-object-missing-interface-lib-pc.json` |

Command:

```sh
TSZ_PERF_COUNTERS=1 /private/tmp/tsz-simple-missing-target/release/tsz \
  --noEmit \
  -p /private/tmp/tsz-bench-fixtures/monorepo-006/tsconfig.json \
  --extendedDiagnostics \
  --pretty false \
  --diagnostics-json docs/plan/perf-runs/raw/2026-05-14-simple-object-missing-interface-lib-diag.json \
  --perf-counters-json docs/plan/perf-runs/raw/2026-05-14-simple-object-missing-interface-lib-pc.json
```

The process exited with status 2 because the generated fixture still reports
diagnostics. Both JSON artifacts were written and parsed successfully.

## Counter Result

| Metric | Count |
| --- | ---: |
| diagnostics | 10,198 |
| files / lib files | 5337 / 87 |
| total / check | 73.54s / 71.38s |
| peak RSS | 3.47 GiB |
| `checker.compute_type_of_symbol_interface_simple_object_fastpath_hits` | 24,762 |
| `compute_type_of_symbol_interface_simple_object_outcomes.success` | 24,762 |
| `reject_out_of_arena_decl` | 6 |
| `reject_missing_interface_decl` | 0 |
| `reject_non_primitive_annotation` | 0 |
| `delegate.misses` | 2 |
| `checker.with_parent_cache_constructed` | 2 |

Remaining declaration/provenance rows:

| Outcome | Symbol | Declarations | Count |
| --- | --- | ---: | ---: |
| `reject_out_of_arena_decl` | `ArrayIterator` | 1 | 1 |
| `reject_out_of_arena_decl` | `CollatorOptions` | 1 | 1 |
| `reject_out_of_arena_decl` | `DateTimeFormatOptions` | 3 | 1 |
| `reject_out_of_arena_decl` | `IteratorObject` | 3 | 1 |
| `reject_out_of_arena_decl` | `NumberFormatOptions` | 3 | 1 |
| `reject_out_of_arena_decl` | `StringIterator` | 1 | 1 |

This run is attribution-mode for counters, so no timing claim is made.

## Decision

The missing-interface declaration/provenance family is exhausted without a
general guard relaxation. The remaining simple-object declaration/provenance
work is now the out-of-arena family, which should stay separate because those
rows require arena-provenance handling rather than name-only lib lookup.
