# fix(checker): mark all cycle members in cross-file circular alias detection

- **Date**: 2026-05-01
- **Branch**: `fix/checker-cross-file-circular-alias-mark-cycle`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

`circular2.ts` (`/a.ts: type A = B` + `/b.ts: type B = A`) expects two TS2456
diagnostics — one per circular alias declaration. Today tsz only emits one
(at the alias whose checker first detects the cycle, typically `/a.ts:A`).

`check_cross_file_circular_type_aliases` walks the lazy chain via
`is_cross_file_circular_alias`, but only marks the entry symbol's `DefId` as
circular in the shared `DefinitionStore`. When the sibling file's checker
later runs the same pass, the mate symbol's `DefId` is not flagged
(`shared_circular = false`) and the alias's resolved type is no longer
`Lazy(...)`, so the function returns early and skips the second emission.

This PR threads the visited cycle members out of the walk and marks all of
them as circular in `DefinitionStore`. Each per-file run of the post-pass
then sees `shared_circular = true` for its local cycle member and emits its
own TS2456. `circular4.ts` (the namespaced variant) already passes; the fix
unblocks the bare-alias pattern.

## Files Touched

- `crates/tsz-checker/src/state/type_analysis/computed_helpers.rs` (~30 LOC change)
- `crates/tsz-checker/tests/<TBD>.rs` (unit test locking the multi-emit behavior)

## Verification

- `./scripts/conformance/conformance.sh run --filter "circular2" --verbose` flips to PASS
- `./scripts/conformance/conformance.sh run --filter "circular"` does not regress (33→34/36)
- `cargo nextest run -p tsz-checker --lib` clean
