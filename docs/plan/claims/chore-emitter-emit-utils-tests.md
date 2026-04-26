# chore(emitter/tests): replace stale emit_utils tests with real coverage

- **Date**: 2026-04-26
- **Branch**: `chore/emitter-emit-utils-tests`
- **PR**: #1316
- **Status**: ready
- **Workstream**: 8.4 (Test coverage / DRY emitter helpers)

## Intent

The `crates/tsz-emitter/tests/emit_utils.rs` file contains stale test stubs
for `push_u64` / `push_usize` / `push_i64` helpers that were removed from
the source module long ago. The file is included via
`#[path = "../../tests/emit_utils.rs"] mod tests;` inside
`crates/tsz-emitter/src/transforms/emit_utils.rs`, but currently only tests
local copies of those defunct helpers and exercises none of the actual
`pub(crate)` functions in the parent module.

This PR replaces the stale stubs with real unit tests for several pure
helpers in `emit_utils.rs`:

- `is_valid_identifier_name` (no current direct tests; only used via
  callers in module emission).
- `next_temp_var_name` (no current tests).
- `skip_trivia_forward` (no current tests).

The change is purely additive in behavior — no source code changes — and
removes a 50-line dead-test maintenance hazard.

## Files Touched

- `crates/tsz-emitter/tests/emit_utils.rs` (~140 LOC of new tests; old
  ~50 LOC of dead tests removed)

## Verification

- `cargo nextest run -p tsz-emitter` — 1557 tests pass (28 new tests in
  `transforms::emit_utils::tests` covering `is_valid_identifier_name`
  (8 tests), `next_temp_var_name` (4 tests), `skip_trivia_forward`
  (15 tests)).
