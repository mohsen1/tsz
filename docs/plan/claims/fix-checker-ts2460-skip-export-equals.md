# fix(checker): skip `"export="` synthetic key in TS2460 renamed-export check

- **Date**: 2026-04-28
- **Branch**: `fix/checker-ts2460-skip-export-equals`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic conformance — module-import diagnostics)

## Intent

`check_symbol_in_binder` walks the target module's exports table looking for a renamed export of the imported name. The binder stores `export = Foo;` as a synthetic export under the key `"export="`. When the imported name `Foo` matched the symbol's declarations, this loop returned `(true, Some("export="))` and the caller emitted TS2460 with the literal text `"exported as 'export='"` — duplicating (and contradicting) the canonical TS2497/TS2616/TS2595/TS2597 export-equals diagnostic that already fires on the same import.

A guard further down (`has_export_equals` → `return None`) was supposed to suppress TS2459/TS2460 in the export-equals case, but it was unreachable because the loop returned earlier.

Fix: skip the `"export="` key in both renamed-export loops (`module_keys` and `file_name`). The guard at the bottom now correctly suppresses TS2459/TS2460 and lets the TS2497/TS2616 path emit the right diagnostic.

Surfaced by `compiler/importNonExportedMember5.ts` and `compiler/importNonExportedMember9.ts` (extra TS2460 with empty `extra_codes` in offline data; live conformance shows TS2460 on top of TS2497/TS2616). Both move from FAIL to PASS.

## Files Touched

- `crates/tsz-checker/src/declarations/import/core/import_members.rs` — skip `"export="` in the two renamed-export loops inside `check_symbol_in_binder` (~10 LOC).

## Verification

- `cargo nextest run -p tsz-checker -E 'test(test_named_import_of_export_equals_target_skips_ts2459_ts2460)'` — already-existing lock test passes (was previously failing).
- `./scripts/conformance/conformance.sh run --filter "importNonExportedMember"` — 13/13 PASS (was 11/13).
- `./scripts/conformance/conformance.sh run --filter "exportAssignment"` — 32/32 PASS (no regression).
