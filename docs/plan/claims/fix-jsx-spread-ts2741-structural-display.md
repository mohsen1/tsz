# fix(checker): JSX spread TS2741 shows structural type form, not alias name

- Date: 2026-04-30
- Branch: claude/exciting-keller-L4bfQ
- Status: shipped (PR #1860 — `fix(checker): JSX spread TS2741 shows structural type form, not alias name`, merged 2026-04-30)

## Problem

In `check_spread_property_types`, the TS2741/TS2739/TS2740 missing-property
diagnostics used `format_type(spread_type)` which returns the type alias name
(e.g. `ComponentProps`) instead of the structural form tsc shows
(`{ property1: string; property2: number; }`).

## Fix

Format `spread_name` directly from `spread_shape.properties` (already
resolved in scope) with `normalize_display_property_order` to ensure
declaration-order output, matching tsc's structural display exactly.

## Files Changed

- `crates/tsz-checker/src/checkers/jsx/spread.rs` — main fix
- `crates/tsz-checker/tests/jsx_component_attribute_tests.rs` — unit test

## Test

`test_spread_ts2741_shows_structural_form_not_alias_name` in
`jsx_component_attribute_tests.rs`.
