# fix(parser): suppress TS1090 on invalid modifiers preceding a `this` parameter

- **Date**: 2026-04-29
- **Branch**: `fix/conformance-roadmap-1777413250`
- **PR**: #1696
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

`parse_parameter_modifiers` (`crates/tsz-parser/src/parser/state_statements_class.rs`)
emits TS1090 ("'X' modifier cannot appear on a parameter") for invalid parameter
modifiers like `async`. When the parameter name turns out to be `this`, tsc
emits *only* TS1433 ("Neither decorators nor modifiers may be applied to 'this'
parameters"); tsz currently emits TS1090 first, and the subsequent TS1433 is
swallowed by `parse_error_at`'s same-start-position dedup at
`crates/tsz-parser/src/parser/state.rs:1107-1116`.

Fix: peek through the modifier run from `parse_parameter` to detect when the
parameter name is `this`, and suppress per-modifier TS1090 emissions in that
case. TS1433 then lands cleanly and matches tsc's diagnostic shape.

Sub-issue #1 of `thisTypeInFunctionsNegative.ts` (Workstream 1 conformance).
That test has additional unrelated parser-recovery sub-issues (lines 172-175,
178-181) and checker-side fingerprint mismatches (TS2353/TS2554/TS2684 at
lines 50-84) that need separate PRs; this PR addresses only the
`function modifiers(async this: C)` case at source line 171.

## Files Touched

- `crates/tsz-parser/src/parser/state_statements_class.rs` (~30 LOC: lookahead helper + signature change on `parse_parameter_modifiers`).
- `crates/tsz-parser/src/parser/state_types_jsx.rs` (1-line caller update).
- `crates/tsz-parser/tests/this_param_modifier_tests.rs` (new test file, ~60 LOC).
- `crates/tsz-parser/src/parser/mod.rs` (1-line module mount for the new test file).

## Verification

- `cargo nextest run -p tsz-parser` — full parser suite green.
- `./scripts/conformance/conformance.sh run --filter "thisTypeInFunctionsNegative" --verbose` — confirms TS1433 now appears at the expected position; remaining mismatches in this multi-issue test are unaffected by this PR.
