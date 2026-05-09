# fix(cli): tsc-parity reasons for `--explainFiles` files-list and default libs (#3901)

- **Date**: 2026-05-08
- **Branch**: `fix/explain-files-tsc-parity-3901`
- **PR**: TBD
- **Status**: claim
- **Workstream**: CLI parity

## Intent

`--explainFiles` was emitting generic reasons that didn't match tsc's
output. Issue #3901 documents three gaps:

1. tsconfig `files`-list entries reported as "Matched by include
   pattern '**/*'" instead of "Part of 'files' list in tsconfig.json".
2. Default-target libs reported as "Library file" instead of "Default
   library for target 'es2018'".
3. Imported files reported as "Imported from '<import>'" instead of
   "Imported via 'specifier' from file 'a.ts'".

This PR ships fixes 1 and 2. Fix 3 needs the resolver to track per-file
import edges (specifier + importer) and is left for a follow-up.

## Files Touched

- `crates/tsz-cli/src/driver/core.rs`:
  - Add `FileInclusionReason::FilesListEntry` and
    `FileInclusionReason::DefaultLibrary(String)` variants.
  - Thread the resolved target into `build_file_infos` and consult
    tsconfig `files` to disambiguate the new variant from the existing
    `IncludePattern` reason.
  - Add `script_target_display_for_explain_files` and
    `is_default_lib_for_target` helpers.
- 4 new unit tests under `explain_files_reason_tests`.

## Verification

- `cargo nextest run -p tsz-cli --lib -E 'test(explain_files_reason)'` —
  4/4 pass.
- Manual repro from issue #3901: `a.ts` now reads "Part of 'files' list
  in tsconfig.json" instead of "Matched by include pattern '**/*'".
- Imported-via case still says `Imported from '<import>'` (out of scope
  for this PR; documented above).
