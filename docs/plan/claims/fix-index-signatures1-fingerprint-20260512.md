# fix(checker): align indexSignatures1 conformance fingerprint

- **Date**: 2026-05-12
- **Branch**: `fix/index-signatures1-fingerprint-20260512`
- **PR**: #5685
- **Status**: wip
- **Workstream**: 1 (Diagnostic conformance)

## Intent

Investigate and fix the remaining `indexSignatures1` conformance fingerprint-only mismatch on latest `main`.
The target is a root-cause checker/solver/display fix with focused regression coverage, not a snapshot-only change.

## Current findings

- Fixed display-boundary loss where string-bucket index signatures always printed `string` instead of their stored key type (`symbol`, branded strings, template patterns).
- Added named-shape index key coverage so interface pattern index signatures reject narrower source keys where TypeScript does.
- Remaining blocker: the solver shape can represent only one string-bucket index signature, but this case needs multiple/template/branded index signatures to stay distinct for contextual typing, element access, excess property display, and object-literal assignability.

## Verification

- `cargo test -p tsz-solver format_object_with_symbol_keyed_index_signature -- --nocapture` passed.
- `cargo build --profile dist-fast -p tsz-cli -p tsz-conformance` passed.
- `.target/dist-fast/tsz-conformance --test-dir /Users/mohsen/code/tsz/TypeScript/tests/cases --cache-file /Users/mohsen/code/tsz/scripts/conformance/tsc-cache-full.json --tsz-binary .target/dist-fast/tsz --filter indexSignatures1 --print-test --print-fingerprints --verbose` still fails as fingerprint-only; remaining mismatches require multi-index-signature representation work.
