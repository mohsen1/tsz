# Route WASM compiler options through shared defaults

- **Date**: 2026-05-04
- **Branch**: `codex/wasm-shared-checker-defaults`
- **PR**: #2704
- **Status**: ready
- **Workstream**: Architecture cleanup

## Intent

Make the `tsz-core` WASM compiler-option adapter start from
`CheckerOptions::default()` and override only the options supplied through the
WASM API. This removes the duplicate full `CheckerOptions` literal from the
WASM layer and keeps strict-family defaults, JSX defaults, JSON/JS defaults,
side-effect import defaults, and iterator-return defaults owned by
`tsz-common`.

## Files Touched

- `crates/tsz-core/src/api/wasm/compiler_options.rs`

## Verification

- `cargo test -p tsz-core compiler_options -- --nocapture`
- `cargo fmt --check`
