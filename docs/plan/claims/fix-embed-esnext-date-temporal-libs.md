# Claim: Embed esnext.date and esnext.temporal lib files

**Branch**: claude/exciting-keller-XFjOB
**Date**: **2026-04-27 05:45:00**
**Status**: In progress

## Problem

The conformance test `TypeScript/tests/cases/compiler/temporal.ts` was failing with:
- **Extra TS6046**: "Argument for '--lib' option must be: ..." — because `esnext.date` and
  `esnext.temporal` were not in `VALID_LIB_VALUES`, causing tsz to reject them as unknown lib names.
- This cascaded into extra TS2345 false positives (tsz couldn't load the Temporal types).

## Root Cause

1. `VALID_LIB_VALUES` in `crates/tsz-core/src/config/mod.rs` was missing `"esnext.date"` and
   `"esnext.temporal"`.
2. Neither lib was embedded as a lib asset in `embedded_libs/mod.rs`.

## Fix

- Created `lib-assets/esnext.date.d.ts` and `lib-assets/esnext.temporal.d.ts` (sourced from
  `TypeScript/lib/lib.esnext.date.d.ts` and `TypeScript/lib/lib.esnext.temporal.d.ts`).
- Created comment-stripped variants in `lib-assets-stripped/`.
- Added both files to `get_lib_content()`, `ALL_LIB_FILENAMES`, and updated `LIB_FILE_COUNT`
  from 103 to 105.
- Added `"esnext.date"` and `"esnext.temporal"` to `VALID_LIB_VALUES` and `VALID_LIB_DISPLAY`.

## Tests Added

- `embedded_libs::tests::test_esnext_date_lib_embedded` — verifies content + `toTemporalInstant`
- `embedded_libs::tests::test_esnext_temporal_lib_embedded` — verifies `Temporal` namespace
- `config::tests::test_esnext_date_and_temporal_are_valid_lib_values` — VALID_LIB_VALUES check
- `config::tests::test_esnext_date_temporal_lib_in_tsconfig_no_ts6046` — no TS6046 emitted

## Remaining Issues (not in scope for this PR)

- **TS2345 false positives**: tsz emits incorrect assignability errors for Temporal option types
  (e.g., `{ timeZone: string }` to `InstantToStringOptions`). Pre-existing, root cause is in
  solver generic instantiation.
- **Missing TS2552**: tsc emits TS2552 from within lib files for undefined `DateTimeFormatPart`.
  Requires tsz to type-check lib files internally.
