# 2026-05-14 Simple-Object Missing-Interface Lib Attribution

Follow-up to
[`2026-05-14-simple-object-provenance-residues.md`](2026-05-14-simple-object-provenance-residues.md).

## Change

With perf counters enabled, the simple local-interface object shortcut now uses
actual/cloned lib symbol provenance to classify the named
`reject_missing_interface_decl` residue family as lib-backed before recording
the reject. After conformance probing on current main, this attribution cleanup
is limited to the non-iterator rows:

- `PropertyDescriptor`
- `PropertyDescriptorMap`
- `RegExpIndicesArray`

Normal checker execution leaves this path disabled with perf counters off. The
semantic lib-type return path remains limited to the pre-existing
out-of-arena/lib-symbol cases; these missing-interface rows still fall through
to the existing full merge path. Iterator-family missing-interface rows stay in
the residue table for a separate conformance-proven slice.

## Reproducer

| Item | Value |
| --- | --- |
| Base | `origin/main` at `1187c59113` |
| Fixture | `scripts/bench/scale-cliff/fixtures/monorepo-006` |
| Build | `CARGO_TARGET_DIR=/private/tmp/tsz-simple-missing-target CARGO_INCREMENTAL=0 cargo build -p tsz-cli --bin tsz --release --features perf-tools` |
| Counter mode | `TSZ_PERF_COUNTERS=1` |
| Raw diagnostics | `docs/plan/perf-runs/raw/2026-05-14-simple-object-missing-interface-lib-diag.json` |
| Raw counters | `docs/plan/perf-runs/raw/2026-05-14-simple-object-missing-interface-lib-pc.json` |

Command:

```sh
TSZ_PERF_COUNTERS=1 /private/tmp/tsz-simple-missing-target/release/tsz \
  --noEmit \
  -p scripts/bench/scale-cliff/fixtures/monorepo-006/tsconfig.json \
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
| total / check | 59.94s / 58.19s |
| peak RSS | 3.49 GiB |
| `checker.compute_type_of_symbol_interface_simple_object_fastpath_hits` | 24,762 |
| `compute_type_of_symbol_interface_simple_object_outcomes.success` | 24,762 |
| `reject_out_of_arena_decl` | 6 |
| `reject_missing_interface_decl` | 4 |
| `reject_non_primitive_annotation` | 0 |
| `delegate.misses` | 2 |
| `checker.with_parent_cache_constructed` | 2 |

Remaining declaration/provenance rows:

| Outcome | Symbol | Declarations | Count |
| --- | --- | ---: | ---: |
| `reject_missing_interface_decl` | `Iterable` | 1 | 1 |
| `reject_missing_interface_decl` | `IteratorReturnResult` | 1 | 1 |
| `reject_missing_interface_decl` | `IteratorYieldResult` | 1 | 1 |
| `reject_missing_interface_decl` | `RegExpStringIterator` | 1 | 1 |
| `reject_out_of_arena_decl` | `ArrayIterator` | 1 | 1 |
| `reject_out_of_arena_decl` | `CollatorOptions` | 1 | 1 |
| `reject_out_of_arena_decl` | `DateTimeFormatOptions` | 3 | 1 |
| `reject_out_of_arena_decl` | `IteratorObject` | 3 | 1 |
| `reject_out_of_arena_decl` | `NumberFormatOptions` | 3 | 1 |
| `reject_out_of_arena_decl` | `StringIterator` | 1 | 1 |

This run is attribution-mode for counters, so no timing claim is made.

## Decision

The non-iterator missing-interface declaration/provenance residues are removed
from attribution without a semantic shortcut or general guard relaxation. The
remaining simple-object declaration/provenance work is the iterator-family
missing-interface tail plus the out-of-arena family; both should stay separate
because they need conformance-specific iterator/generic and arena-provenance
handling rather than a broad name-only lib lookup.
