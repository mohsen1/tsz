# fix(checker): emit TS7008 for null JS constructor provisional members

- **Date**: 2026-05-12
- **Branch**: `fix/js-null-constructor-ts7008-20260512`
- **PR**: TBD
- **Status**: ready
- **Workstream**: 1 (Diagnostic conformance)

## Intent

Restore `typeFromJSInitializer` conformance by emitting TS7008 for checked-JS constructor members initialized with `null` under `noImplicitAny`, while preserving the open `any` write surface for later assignments.

## Verification

- `cargo test -p tsz-checker test_plain_js_function_constructor_provisional_initializers_emit_ts7008_in_check_js -- --nocapture` passed.
- `cargo test -p tsz-checker test_plain_js_function_constructor_implicit_any_properties_keep_any_write_surface -- --nocapture` passed.
- `cargo build --profile dist-fast -p tsz-cli -p tsz-conformance` passed.
- `.target/dist-fast/tsz-conformance --test-dir /Users/mohsen/code/tsz/TypeScript/tests/cases --cache-file scripts/conformance/tsc-cache-full.json --tsz-binary .target/dist-fast/tsz --filter typeFromJSInitializer --print-fingerprints --verbose` passed: 4/4, fingerprint-only 0.
