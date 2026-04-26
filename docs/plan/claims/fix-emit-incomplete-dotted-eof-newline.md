# fix(emit): only insert newline after recovery dot when source had one

- **Date**: 2026-04-26
- **Branch**: `fix/emit-incomplete-dotted-eof-newline`
- **PR**: TBD (drafting)
- **Status**: ready
- **Workstream**: 2 (Emit pass-rate)

## Intent

Fix `incompleteDottedExpressionAtEOF.ts` JS emit. When the parser recovers
from a missing identifier after `.` (e.g. `var p2 = window. ` at EOF), the
emitter previously always wrote a newline after the dot, producing
`window.\n;`. tsc only breaks the line when the original source had a
newline between the dot and the next significant token. With no newline in
the source (EOF case), tsc emits `window.;` on a single line; with a
newline (e.g. `bar.\n}`), tsc preserves the break and emits `bar.\n    ;`.

The fix inspects raw source bytes between the receiver-expression's end
and the property-access node's end. If a `\n` is present, we keep the
existing line break; otherwise we leave the synthetic trailing token on
the same line, matching tsc.

## Files Touched

- `crates/tsz-emitter/src/emitter/expressions/access.rs` (+18 / -3 LOC)
- `crates/tsz-emitter/Cargo.toml` (+4 LOC, register the new test target)
- `crates/tsz-emitter/tests/property_access_recovery_tests.rs` (new, ~104 LOC)

## Verification

- `cargo nextest run -p tsz-emitter --test property_access_recovery_tests` (3/3 pass)
- `cargo nextest run -p tsz-emitter` (1656/1656 pass, 2 skipped)
- `./scripts/emit/run.sh --filter=incompleteDottedExpressionAtEOF` (1/1 pass — was failing pre-fix with `+2/-1 lines` diff)
- `./scripts/emit/run.sh --filter=parse1` (1/1 pass — no regression on the `bar.\n}` case)
- `./scripts/emit/run.sh --filter=classAbstractCrashedOnce` (1/1 pass — no regression on the `this.\n}` case)
