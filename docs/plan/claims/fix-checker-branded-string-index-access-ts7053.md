# fix(checker): report TS7053 for branded string index mismatches

- **Date**: 2026-05-05
- **Branch**: `fix/checker-branded-string-index-access-ts7053`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic conformance)

## Intent

Close the branded-string indexed-access slice of the `indexSignatures1`
conformance mismatch. `tsz` currently rejects intersection-branded string
keys such as `string & Tag1` with `TS2538` before normal indexed-access
compatibility can run, while `tsc` treats them as string-like keys and reports
`TS7053` only when the branded key is not accepted by the target index
signature. This PR will keep the fix in the checker/solver boundary for index
key classification and add a focused Rust regression test.

## Files Touched

- `crates/tsz-checker/src/state/state_checking/indexed_access.rs` (planned)
- `crates/tsz-checker/tests/*` (planned regression test)

## Verification

- Planned: `cargo check --package tsz-checker`
- Planned: `cargo nextest run --package tsz-checker --lib`
- Planned: `./scripts/conformance/conformance.sh run --filter "indexSignatures1" --verbose`
