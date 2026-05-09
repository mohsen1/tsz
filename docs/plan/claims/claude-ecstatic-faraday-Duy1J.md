# fix(checker): emit TS1262 for every top-level `await` declaration in a module

- **Date**: 2026-05-05
- **Branch**: `claude/ecstatic-faraday-Duy1J`
- **PR**: TBD
- **Status**: ready
- **Workstream**: checker conformance (TS1262)

## Intent

`check_reserved_await_identifier_in_module` had a `break` after reporting
the first TS1262 diagnostic, causing only one error per file regardless of
how many illegal top-level `await` identifiers existed. Removing the `break`
lets the loop report a TS1262 at every qualifying declaration, matching tsc's
behavior (e.g., `const await = 1; let await = 2; var await = 3;` → 3×TS1262).

Closes #2816.

## Files Touched

- `crates/tsz-checker/src/state/state_checking/source_file.rs` (1 LOC removed)
- `crates/tsz-checker/tests/ts1262_multiple_await_declarations_tests.rs` (new test file)
- `crates/tsz-checker/Cargo.toml` (register new test target)

## Verification

- `cargo test -p tsz-checker --test ts1262_multiple_await_declarations_tests` — 3 tests pass
- No regressions in checker lib tests (3362 passed, 1 pre-existing failure unrelated to this change)
