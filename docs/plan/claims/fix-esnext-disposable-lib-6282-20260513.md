# fix(cli): load esnext disposable symbols from --lib esnext

- **Date**: 2026-05-13
- **Branch**: `fix-esnext-disposable-lib-6282-20260513`
- **PR**: #6285
- **Status**: ready
- **Workstream**: 1 (Diagnostic conformance / lib compatibility)

## Intent

Issue #6282 reports that `--lib esnext` still emits TS2339 for `Symbol.dispose` and `Symbol.asyncDispose` in using declarations. The lib asset already contains `esnext.disposable.d.ts`, so this claim targets lib resolution/loading rather than changing the declaration text. The fix should keep lib ownership in config/driver loading and add a focused CLI regression for the reported repro.

## Files Touched

- `crates/tsz-cli/src/driver/core.rs` (expected, lib resolution if needed)
- `crates/tsz-core/src/config/mod.rs` or lib-resolution tests (expected, if transitive expansion is the root cause)
- `crates/tsz-cli/tests/tsc_compat_tests.rs` or driver tests (expected regression)

## Verification

- `cargo run -p tsz-cli --bin tsz -- --noEmit --strict --lib esnext /tmp/issue6282.ts` (pass)
- `cargo test -p tsz-cli --test tsc_compat_tests esnext_lib_loads_disposable_symbols_without_builtin_lib_diagnostics -- --nocapture` (1 passed)
- `cargo test -p tsz-cli collect_diagnostics_ -- --nocapture` (20 passed, 1 ignored)
- `cargo fmt --all -- --check` (pass)
- `git diff --check` (pass)
