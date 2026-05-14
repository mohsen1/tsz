# 2026-05-14 Simple-Object Missing-Interface Lib Resolution Probe

Follow-up to
[`2026-05-14-simple-object-provenance-residues.md`](2026-05-14-simple-object-provenance-residues.md).

## Probe

The simple local-interface object shortcut was tested with a behavior change
that routed named `reject_missing_interface_decl` residue rows through existing
lib metadata before recording the reject. A broad probe included iterator and
non-iterator rows. A narrowed probe kept only the apparent non-iterator rows:

- `PropertyDescriptor`
- `PropertyDescriptorMap`
- `RegExpIndicesArray`

The path used `resolve_lib_type_by_name` and, for the explicit
missing-interface admission path, `resolve_lib_type_with_params` as fallback.
It did not manually lower declaration arenas. CI showed that this admission is
not safe, so the behavior change is not part of the mergeable result.

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

## Counter Result From Unsafe Probe

| Metric | Count |
| --- | ---: |
| diagnostics | 10,198 |
| files / lib files | 5337 / 87 |
| total / check | 60.45s / 58.78s |
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

This run is attribution-mode for counters, so no timing claim is made. The
counter result is useful only as evidence for the rejected probe; it is not a
production behavior claim.

## CI Result

Both behavior variants failed current-main CI:

| Variant | Run | Result |
| --- | --- | --- |
| Broad allowlist | `25849166412` | emit and conformance aggregate failed |
| Narrowed allowlist | `25850104213` | emit and conformance aggregate failed |

The narrowed run failed DTS emit at `1477 < 1527` and conformance aggregate at
`12575/12585 < 12581/12585`. Newly failing cases included:

- `TypeScript/tests/cases/compiler/deepKeysIndexing.ts`
- `TypeScript/tests/cases/compiler/deeplyNestedMappedTypes.ts`
- `TypeScript/tests/cases/compiler/excessivelyLargeTupleSpread.ts`
- `TypeScript/tests/cases/compiler/inferFromAnnotatedReturn1.ts`
- `TypeScript/tests/cases/compiler/inferFromGenericFunctionReturnTypes3.ts`
- `TypeScript/tests/cases/compiler/modularizeLibrary_NoErrorDuplicateLibOptions1.ts`
- `TypeScript/tests/cases/compiler/modularizeLibrary_NoErrorDuplicateLibOptions2.ts`
- `TypeScript/tests/cases/compiler/modularizeLibrary_TargetES5UsingES6Lib.ts`
- `TypeScript/tests/cases/compiler/ramdaToolsNoInfinite2.ts`
- `TypeScript/tests/cases/compiler/strictOptionalProperties1.ts`

## Decision

Do not merge a string allowlist that resolves missing-interface lib rows through
generic lib-name lookups. The observed regressions mean the shortcut cannot
prove that the symbol being computed is equivalent to the canonical lib shape
it reuses, even when the symbol appears to come from actual or cloned lib
metadata.

The bigger design should make lib reuse identity-driven:

- Key the decision by canonical lib symbol identity or stable declaration
  provenance, not by escaped name.
- Preserve generic parameters, defaults, substitutions, and instantiated result
  identity when reusing lib shapes.
- Distinguish actual lib declarations, cloned lib declarations, user imports,
  and interface augmentation before bypassing merge paths.
- Add conformance-focused guards for modularized libs, deep mapped/indexed
  types, generic function return inference, and strict optional properties
  before reopening this shortcut.
