# fix(solver): gate test_tracing inline tests on debug_assertions

- **Date**: 2026-05-02
- **Branch**: `fix/test-tracing-release-build`
- **PR**: TBD
- **Status**: ready
- **Workstream**: dev experience (release-mode test build was red on main)

## Intent

Five inline tests in `crates/tsz-solver/src/diagnostics/format/test_tracing.rs`
fail under `cargo test --release -p tsz-solver --lib`:

```
diagnostics::format::test_tracing::tests::capture_basic_event
diagnostics::format::test_tracing::tests::capture_basic_span
diagnostics::format::test_tracing::tests::capture_is_isolated_between_tests
diagnostics::format::test_tracing::tests::capture_nested_spans
diagnostics::format::test_tracing::tests::capture_subtype_check_span
```

Root cause: the workspace `tracing` dep is configured with feature
`release_max_level_warn` (root `Cargo.toml`), so `tracing::debug!`,
`tracing::debug_span!`, and `tracing::trace_span!` expand to no-ops when
`debug_assertions` is off. The capture layer therefore never sees the events
the tests assert on. `check_subtype` is similarly gone because it's wrapped
in `tracing::trace_span!`. The five tests can only ever pass in a build
where `debug_assertions = on`.

Fix: gate the entire `test_tracing` submodule on `cfg(all(test,
debug_assertions))` in `crates/tsz-solver/src/diagnostics/format/mod.rs`.
The capture utility was already `#[cfg(test)]`-only and has no in-tree
callers outside its own inline `#[cfg(test)] mod tests` block, so removing
it from release-test builds doesn't affect any production code or other
tests. Debug-mode `cargo test` still compiles and runs all five tests.

## Files Touched

- `crates/tsz-solver/src/diagnostics/format/mod.rs` (+8/-1): widen the
  `cfg(test)` on `pub mod test_tracing` to `cfg(all(test,
  debug_assertions))`, with a comment explaining the
  `release_max_level_warn` interaction.

## Verification

- `cargo test -p tsz-solver --lib` — 5581 pass, 0 fail (the 5
  `test_tracing::tests::*` tests are present and pass).
- `cargo test --release -p tsz-solver --lib` — 5570 pass, 0 fail (the 5
  `test_tracing::tests::*` tests are gated out — no longer red).
- `cargo clippy -p tsz-solver --lib --tests --all-targets -- -D warnings` —
  clean in both `dev` and `release`.
- `cargo fmt --all --check` — clean.
- No production-code change → conformance unchanged (12339/12582).

## Notes

The non-test public surface of `test_tracing` (`with_test_tracing`,
`TracingCapture`, `CapturedSpan`, `CapturedEvent`) is unused outside this
module today. If a future caller wants to use it from a non-debug-assertions
build, the gate can be relaxed; for now this matches the only environment
where the utility actually works.
