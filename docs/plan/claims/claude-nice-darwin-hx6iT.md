# fix(emitter): jsxdev columnNumber counts UTF-16 code units

- **Date**: 2026-05-06
- **Branch**: `claude/nice-darwin-hx6iT`
- **PR**: TBD
- **Status**: claim
- **Workstream**: emitter parity (issue #3994)

## Intent

`react-jsxdev` emits the JSX element's source column for the `columnNumber`
field of the `jsxDEV` source object. tsz currently increments the column once
per UTF-8 byte before the element, so non-ASCII content (emoji, accented
letters, etc.) preceding a JSX element shifts the emitted column past tsc.
TypeScript reports source columns in UTF-16 code units. Fix
`source_line_col_pos` (the JSX-local helper) to count UTF-16 code units like
the existing source-map line/column helper.

## Files Touched

- `crates/tsz-emitter/src/emitter/jsx/transform.rs` — switch the JSX dev mode
  line/column helper to iterate `chars` and count `len_utf16()`.
- `crates/tsz-emitter/tests/jsx_spread_tests.rs` — add a regression test that
  checks `columnNumber` for an emoji-prefixed `react-jsxdev` source.

## Verification

- `cargo nextest run -p tsz-emitter`
