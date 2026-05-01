# fix(checker): mark all cycle members in cross-file circular alias detection

- **Date**: 2026-05-01
- **Branch**: `fix/checker-cross-file-circular-alias-mark-cycle`
- **PR**: #1965
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

`circular2.ts` (`/a.ts: type A = B` + `/b.ts: type B = A`) expects two TS2456
diagnostics — one per circular alias declaration. tsz emitted only one (at
`/a.ts:A`), because `check_cross_file_circular_type_aliases` gated the
`shared_circular` path on `!is_lazy`. The sibling file's resolved type for
`B` is `Lazy(A's def_id)` (cross-file resolution returns a `Lazy`
placeholder) AND the inline detection in `is_direct_circular_reference`
already marks B's `DefId` circular while resolving A. The `!is_lazy` guard
short-circuited shared_circular even though the marking was present, so
`/b.ts`'s post-pass never reached the emit branch and the lazy-chain walk
also returned `None` (A's body in DefinitionStore is a self-loop placeholder).

Fix: drop the `!is_lazy &&` guard from `shared_circular`. When a sibling
file's inline detection has already marked our `DefId` circular, emit
TS2456 for our local declaration regardless of whether the cached resolved
type is still a `Lazy(...)` placeholder.

## Files Touched

- `crates/tsz-checker/src/state/type_analysis/computed_helpers.rs` (1-line behavior change + comment refresh)

## Verification

- `./scripts/conformance/conformance.sh run --filter "circular2" --verbose` flips to PASS.
- `./scripts/conformance/conformance.sh run --filter "circular"` 33/36 → 34/36; remaining 2 failures are pre-existing TS2589 / TS2564 cases unrelated to this PR.
- `cargo nextest run -p tsz-checker --lib` clean (3078 / 3078).
- `cargo nextest run -p tsz-solver --lib` clean (5567 / 5567).
- Conformance test serves as the multi-file regression lock; a unit-test
  harness for two `check_source_file` passes against a single shared
  `DefinitionStore` is non-trivial and out of scope for this slice.

