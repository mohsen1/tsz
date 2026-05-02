# fix(checker): show `T | undefined` for null-default optional params in TS2345

- **Date**: 2026-05-02
- **Branch**: `claude/brave-thompson-c47JW`
- **PR**: TBD
- **Status**: claim
- **Workstream**: conformance / fingerprint parity

## Intent

When a call argument fails to assign to an optional parameter (`x?: T` or
`x = ...`), tsc renders the externally-visible parameter type. For
underlying types that already carry only nullish members (e.g. `null`
inferred from `function f(x = null)`), tsc keeps the full
`null | undefined` surface. Previously tsz reported only the inner type
(`null`), losing the `| undefined`.

The fix lives entirely in the call-argument display layer. At the
"regular non-spread arg lands on optional non-rest param" path
(`format_call_parameter_type_for_diagnostic`), the param type is
widened with `| undefined` (when the union doesn't already contain
it) before the existing strip-aware formatter runs. The strip helper
then chooses correctly:

- Underlying `null` → widened `null | undefined` → strip would leave the
  union empty, so it declines → display `null | undefined` (matches tsc).
- Underlying `number`/tuple → widened `… | undefined` → strip applies →
  display `number` / `[…]` (also matches tsc, no regression).

The solver remains untouched; the diagnostic surface stays a checker
display concern.

Flips `destructuringParameterDeclaration1ES6` from fingerprint-only to
PASS.

## Files Touched

- `crates/tsz-checker/src/error_reporter/call_errors/display_formatting.rs`
  (~25 LOC: widen-then-strip at the optional-non-rest call-param display
  path)
- `crates/tsz-checker/src/error_reporter/call_errors_tests.rs`
  (+~80 LOC unit tests covering null-default, concrete optional, and
  already-undefined-containing parameters)

## Verification

- `cargo nextest run -p tsz-checker --lib` (3127 pass)
- `cargo nextest run -p tsz-solver --lib` (5579 pass)
- `./scripts/conformance/conformance.sh run --filter destructuringParameterDeclaration1ES6` → 1/1 PASS
- `./scripts/conformance/conformance.sh run --filter emitSkipsThisWithRestParameter` → 1/1 PASS (no regression)
- `cargo fmt --all --check` clean
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean
- Full conformance: TBD (running)
