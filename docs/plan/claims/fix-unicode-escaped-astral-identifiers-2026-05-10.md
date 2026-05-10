# fix(parser): recover unicode escaped astral identifiers

- **Date**: 2026-05-10
- **Branch**: `fix/unicode-escaped-astral-identifiers-2026-05-10`
- **PR**: TBD
- **Status**: ready
- **Workstream**: conformance parser/scanner recovery

## Intent

Bring scanner and parser recovery for astral identifier characters and braced
astral Unicode escapes in line with tsc for `unicodeEscapesInNames02`.
The scanner accepts valid ES2015 astral identifier escapes while preserving ES5
invalid-character recovery, and the parser keeps the recovered token stream
visible enough to land tsc-shaped parse diagnostics instead of cascading at
later anchors.

## Files Touched

- `crates/tsz-scanner/src/scanner_impl.rs` (target-sensitive astral escape scanning)
- `crates/tsz-scanner/tests/scanner_comprehensive_tests.rs` (ES2015/ES5 astral escape coverage)
- `crates/tsz-parser/src/parser/state_declarations.rs` (import/export specifier recovery)
- `crates/tsz-parser/src/parser/state_statements.rs` (statement/declaration recovery)
- `crates/tsz-parser/tests/parser_improvement_tests.rs` (parser recovery coverage)
- `crates/tsz-parser/tests/state_statement_tests.rs` (scanner-shaped statement recovery)

## Verification

- `cargo fmt --all --check`
- `cargo test -p tsz-scanner unicode_escape_braced_astral_identifier_start -- --nocapture`
- `cargo test -p tsz-parser --lib astral_identifier_debris_uses_scanner_shaped_recovery -- --nocapture`
- `.target/dist-fast/tsz-conformance --test-dir TypeScript/tests/cases --cache-file scripts/conformance/tsc-cache-full.json --tsz-binary .target/dist-fast/tsz --filter unicodeEscapesInNames02 --verbose --print-fingerprints --workers 1` (1/1 passed, fingerprint-only 0)
- pre-commit hook: fmt, affected clippy, wasm warning gate, architecture guardrails, and 24,066 affected tests
