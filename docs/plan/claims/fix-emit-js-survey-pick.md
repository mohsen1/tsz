# fix(parser): leave `*` for outer parser in if-body recovery after invalid char

- **Date**: 2026-04-26
- **Branch**: `fix/emit-js-survey-pick`
- **PR**: #1373
- **Status**: ready
- **Workstream**: 2 (JS Emit pass-rate)

## Intent

`MemberFunctionDeclaration8_es6` (input `if (a) ¬ * bar;`) was emitting
`bar;` instead of tsc's `* bar;` because the if-body recovery consumed the
`*` after reporting TS1127/TS1109. Stop consuming the `*` so the outer
parser reparses `* bar;` as a separate expression statement (binary with
missing LHS), matching tsc's emit. Also drop the redundant Asterisk-only
recovery branch so `if (a) * bar;` falls through to `parse_statement`,
which already handles `*` as a binary operator with missing LHS — matching
tsc's `if (a)\n     * bar;`.

## Files Touched

- `crates/tsz-parser/src/parser/state_declarations_exports.rs` (~14 LOC delete, 4 LOC add)
- `crates/tsz-parser/tests/state_statement_tests.rs` (+58 LOC: two new regression tests)

## Verification

- `cargo nextest run -p tsz-parser` — 672 pass, 1 skipped
- `./scripts/conformance/conformance.sh run --filter MemberFunctionDeclaration8_es6` — pass
- `./scripts/emit/run.sh --js-only` — 91.2% (12329/13526), +1 vs baseline
- Full conformance (`scripts/safe-run.sh ./scripts/conformance/conformance.sh run`) — 96.8% (12183/12582), no regressions
