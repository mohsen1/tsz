# fix(checker): align library reference duplicate declaration diagnostics

- **Date**: 2026-05-05
- **Branch**: `fix/checker-library-reference-5-fingerprint`
- **PR**: #2818
- **Status**: ready
- **Workstream**: 1 (Diagnostic conformance)

## Intent

Close the fingerprint-only `library-reference-5` conformance slice. `tsz`
currently emits the same diagnostic code as `tsc` (`TS2403`) but differs in the
duplicate declaration diagnostic fingerprint for library reference handling.

## Files Touched

- `crates/tsz-cli/src/driver/sources.rs`

## Verification

- `cargo fmt --all --check`
- `cargo check --package tsz-cli`
- `cargo test -p tsz-cli read_source_files_preserves_reference_discovery_order -- --nocapture`
- `./scripts/conformance/conformance.sh run --filter "library-reference-5" --verbose` (1/1 passed, 100%, 0 fingerprint-only)
- `./scripts/conformance/conformance.sh run --max 200` (200/200 passed, 100%, 0 fingerprint-only)

`cargo nextest` availability is unknown in this environment; targeted
`cargo test` was used for local verification.

## Notes

`read_source_files` discovered the referenced packages in tsc-compatible order
from the root file (`foo`, then `bar`), but the final source list was sorted
lexicographically by path. That made `bar`'s nested `alpha` declaration appear
before `foo`'s nested `alpha`, reversing the TS2403 "previous" and "current"
types. Preserving BFS discovery order in the read result keeps declaration
ordering aligned with reference traversal while retaining a path fallback for
any unordered entries.
