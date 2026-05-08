# fix(checker): bare-identifier resolution doesn't leak unimported cross-module exports (#3504)

- **Date**: 2026-05-08
- **Branch**: `fix/cross-file-fallback-leak-3504`
- **PR**: TBD
- **Status**: claim
- **Workstream**: name-resolution boundary

## Intent

`b.ts` referencing `leaked` (declared as `export const leaked = 1` in
`a.ts`) without an import should emit TS2304. tsz silently resolved
through the cross-binder fallback and type-checked the call as if the
symbol were a global, because the fallback only rejected
*non-exported* cross-module values.

## Fix

Tighten the cross-binder filter in
`resolve_identifier_with_filter`'s closure (the FIRST resolver pass in
`resolve_identifier_symbol_inner`): reject cross-module values whose
owning file is a true external module (not a declaration file, not a
script, not a global augmentation source) regardless of whether the
symbol is `is_exported`. The class-member branch below the new gate
preserves the TS2663 ("Did you mean 'this.X'?") detector path for
inherited fields, because that flow resolves through the class
hierarchy rather than the raw cross-file lookup.

The same gate already existed in the SECOND fallback path further down
in the resolver but only for non-exported symbols; this PR mirrors it
on the first pass and removes the `!symbol.is_exported` carve-out from
both.

## Files Touched

- `crates/tsz-checker/src/symbols/symbol_resolver.rs` — extend the
  first-pass closure with the `is_cross_module_private` gate; relax
  the second-pass `is_private_external_module_value` filter to drop
  the `is_exported` carve-out.

## Verification

- `cargo nextest run -p tsz-checker` — 7211 / 7211 tests pass.
- Manual repro from #3504: `tsz` now reports
  `b.ts(2,1): error TS2304: Cannot find name 'leaked'.`, matching tsc.
- A multi-file unit test was attempted but the test harness in
  `crates/tsz-checker/tests/` does not faithfully reproduce the
  cross-binder resolver path (the bug needs the full driver-level
  binder merge); the manual CLI repro stands as the verification
  surface for now.
