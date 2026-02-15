# TS2322 Parser Object Fix (2026-02-15)

## Scope
- Focused parser conformance mismatch where `TS2322` was missing for `Object` assignment cases in `parserAutomaticSemicolonInsertion1.ts`.
- Root cause was solver-side `Object`-compatibility logic being too permissive for callable types.

## Root Cause
- `crates/tsz-solver/src/subtype_rules/generics.rs` routes global-`Object` assignability checks through
  `SubtypeChecker::is_global_object_interface_type`.
- That helper used `is_object_keyword_type` as the final fallback, which returns `true` for callable/source types
  (`function_shape` or `callable_shape`), so call signatures were treated as assignable to global `Object`.
- This mismatched TS expectations in parser regression cases like:
  - `interface I { (): void }`
  - `o: Object; o = i;` (should report `TS2322`)
  - `declare var a: { (): void }`
  - `o = a;` (should report `TS2322`)

## Fix
- `is_global_object_interface_type` now explicitly excludes:
  - `function` shapes
  - callable signatures
- All other behavior for global `Object` compatibility remains unchanged.
- File updated:
  - `crates/tsz-solver/src/subtype_rules/intrinsics.rs`

## Verification
- `./scripts/conformance.sh run --error-code 2322 --filter parserAutomaticSemicolonInsertion1 --print-fingerprints`
  - Result: `1/1 passed (100.0%)`
- `./scripts/conformance.sh run --error-code 2322 --filter parser --print-fingerprints`
  - Result: `555/794 passed (69.9%)`
  - `TS2322` no longer appears in parser-top mismatch set for this run.

## Notes
- Remaining parser failures in this slice are now outside `TS2322` (e.g., `TS2304`, `TS1005`, `TS1109`) and follow existing parser-recovery paths.
