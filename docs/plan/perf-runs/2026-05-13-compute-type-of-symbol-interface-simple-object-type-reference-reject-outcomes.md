# 2026-05-13 — `compute_type_of_symbol` Simple-Object `type_reference` Reject Outcomes (deferred)

Status: deferred. The JSON schema field remains, but runtime recording for this
second-level split is disabled in the guarded PR because resolving symbols at
the simple-object reject site perturbed TS2339 receiver display on current
main. Do not use the raw counts below as merge evidence.

Follow-up to:

- `2026-05-13-compute-type-of-symbol-interface-simple-local-object-guarded-rerun.md`
- `2026-05-13-compute-type-of-symbol-interface-simple-object-outcomes.md`

Goal: split `interface_simple_object_non_primitive_annotation_kinds.type_reference`
into actionable reject outcomes without changing shortcut behavior. The first
implementation did not satisfy that requirement, so this is now follow-up work.

## Change

Add `compute_type_of_symbol_interface_simple_object_type_reference_reject_outcomes`
as a named counter array and JSON snapshot field.

Buckets include:

- `identifier_resolvable_symbol`
- `identifier_value_only_symbol`
- `identifier_not_found_symbol`
- `identifier_compiler_managed_type`
- `qualified_name_resolvable_symbol`
- `qualified_name_value_only_symbol`
- `qualified_name_not_found_symbol`
- `other_type_name_syntax`
- `malformed_type_reference`

Intended recording point: the existing `RejectNonPrimitiveAnnotation` site in
the simple-object shortcut, only when the annotation-kind classifier reports
`TypeReference`. The guarded PR leaves this second-level recording disabled
until the classifier is side-effect-free.

## Reproducer

| Item | Value |
| --- | --- |
| Raw artifact | `docs/plan/perf-runs/raw/2026-05-13-compute-type-of-symbol-interface-simple-local-object-guarded-monorepo-006.json` |
| `tsz` build | `CARGO_TARGET_DIR=/Users/mohsen/.cache/tsz-target cargo build -p tsz-cli --bin tsz --release` |
| Fixture | `scripts/bench/scale-cliff/fixtures/monorepo-006` |
| Counter mode | `TSZ_PERF_COUNTERS=1` |
| Command | `/Users/mohsen/.cache/tsz-target/release/tsz --noEmit -p <fixture>/tsconfig.json --extendedDiagnostics --pretty false` |

## Superseded Result

From the run:

- diagnostics: `10,198` (unchanged)
- `compute_type_of_symbol_interface_simple_object_outcomes.success = 0`
- `checker.compute_type_of_symbol_interface_simple_object_fastpath_hits = 0`
- `compute_type_of_symbol_interface_simple_object_outcomes.reject_non_primitive_annotation = 24,760`
- `interface_simple_object_non_primitive_annotation_kinds.type_reference = 24,760`

The attempted `type_reference` reject split reported:

- `identifier_not_found_symbol = 24,760`
- all other reject-outcome buckets: `0`

## Decision

1. Do not relax `type_reference` guards blindly.
2. Any future shortcut expansion here needs a conformance-proven,
   side-effect-free attribution strategy first.
3. If that strategy is not viable, simplify/remove dead shortcut branches.
