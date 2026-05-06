# fix(checker): suppress infer conditional mapped member TS2344

- **Date**: 2026-05-06
- **Branch**: `fix/checker-infer-conditional-constraint-mapped-member`
- **PR**: #3590
- **Status**: abandoned
- **Workstream**: 1 (Diagnostic conformance)
- **Claimed**: 2026-05-06 02:07:12 UTC

## Intent

Fix the current conformance false positive in
`TypeScript/tests/cases/compiler/inferConditionalConstraintMappedMember.ts`.
The picker reports that TypeScript emits no diagnostics, while `tsz` emits an
extra `TS2344`.

## Files Touched

- `docs/plan/claims/fix-checker-infer-conditional-constraint-mapped-member.md`
- implementation files to be identified during root-cause investigation
- owning-crate Rust regression test

## Verification

- `scripts/session/quick-pick.sh`
- `./.target/dist-fast/tsz-conformance --test-dir /Users/mohsen/code/tsz-main-worktree/TypeScript/tests/cases --cache-file scripts/conformance/tsc-cache-full.json --tsz-binary ./.target/dist-fast/tsz --server-binary ./.target/dist-fast/tsz-server --workers 1 --filter inferConditionalConstraintMappedMember --print-test --verbose --print-fingerprints --print-test-files` (`FINAL RESULTS: 1/1 passed (100.0%)`)

## Abandonment Note

The committed snapshot was stale for this target on current `origin/main`.
Direct conformance verification shows the picked test already passes, so this
claim/PR would not increase the pass count.
