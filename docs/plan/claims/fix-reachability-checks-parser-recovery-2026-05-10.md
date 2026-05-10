# fix(parser): align reachability recovery diagnostics

- **Date**: 2026-05-10
- **Branch**: `fix/reachability-checks-parser-recovery-2026-05-10`
- **PR**: TBD
- **Status**: ready
- **Workstream**: conformance parser recovery

## Intent

Fix the fingerprint-only parser recovery drift in
`TypeScript/tests/cases/compiler/reachabilityChecksNoCrash1.ts`.
The goal is to match tsc's TS1xxx recovery anchors without broad parser
fallbacks or test-specific special cases, and to add focused parser tests that
cover the recovery shape behind the conformance case.

## Files Touched

- `crates/tsz-parser/src/parser/state_statements_class.rs` (definite-assignment parameter tail recovery)
- `crates/tsz-parser/tests/state_statement_tests.rs` (exact-anchor parser regression)

## Verification

- `cargo fmt --all --check`
- `git diff --check HEAD`
- `cargo test -p tsz-parser --lib parse_definite_assignment_marker_return_type_reports_statement_recovery -- --nocapture`
- `.target/dist-fast/tsz-conformance --test-dir TypeScript/tests/cases --cache-file scripts/conformance/tsc-cache-full.json --tsz-binary .target/dist-fast/tsz --filter reachabilityChecksNoCrash1 --verbose --print-fingerprints --workers 1` (1/1 passed, fingerprint-only 0)
