# chore(checker): simplify export module-specifier presence check

- **Date**: 2026-05-12
- **Branch**: `codex/cleanup-checker-export-module-presence-20260512`
- **PR**: #5689
- **Status**: ready
- **Workstream**: DRY cleanup

## Intent

Replace an inverted `NodeIndex` sentinel check in checker namespace export
handling with a direct `is_some()` presence check. This keeps the runtime
namespace export classification logic behavior-preserving while making the
condition easier to scan.

## Files Touched

- `crates/tsz-checker/src/types/type_checking/declarations.rs`
- `docs/plan/claims/codex-cleanup-checker-export-module-presence-20260512.md`

## Verification

- `cargo fmt --check` passed.
- `cargo nextest run -p tsz-checker namespace` attempted first and failed during
  local test-binary linking with `No space left on device` before tests ran.
- `cargo nextest run -p tsz-checker --lib namespace` passed: 86 passed, 3758
  skipped.
- `cargo clippy -p tsz-checker --all-targets -- -D warnings` passed.
- Planned CI: unit, conformance, fourslash, emit, lint, dist, wasm, wasm-web
