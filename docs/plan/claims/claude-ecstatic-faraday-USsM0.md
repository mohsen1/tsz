# fix(checker): stop processing triple-slash directives past directive prologue

- **Date**: 2026-05-05
- **Branch**: `claude/ecstatic-faraday-USsM0`
- **PR**: TBD
- **Status**: ready
- **Workstream**: bug fix (issue #2851)

## Intent

TypeScript only recognises `///` directives in the leading prologue before any
executable statement. The four scanner helpers in `triple_slash_validator.rs`
scanned every source line without a prologue boundary, so late directives after
code produced false-positive diagnostics (TS6231, TS2688, TS1084, TS2458). This
PR adds a `past_prologue` guard to all four helpers and locks the behaviour with
seven new unit tests.

## Files Touched

- `crates/tsz-checker/src/triple_slash_validator.rs` (~20 LOC added per function)
- `crates/tsz-checker/tests/triple_slash_validator.rs` (~80 LOC new tests)

## Verification

- `cargo test -p tsz-checker --lib triple_slash` — 32/32 pass
