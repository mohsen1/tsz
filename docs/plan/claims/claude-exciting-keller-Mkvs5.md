# fix: normalize File-not-found message paths for cross-platform TS6053 fingerprint parity

- **Date**: 2026-04-29
- **Branch**: `claude/exciting-keller-Mkvs5`
- **PR**: TBD
- **Status**: ready
- **Workstream**: 1 (Conformance fixes)

## Intent

Triple-slash `/// <reference path="...">` diagnostics (TS6053) were failing
fingerprint comparison for any test that uses Windows-style backslash paths
pointing outside the temp directory. Two compounding problems:

1. `directive.rs` joined Windows backslash paths (`..\..\..`) without
   normalizing them to forward slashes first; on Linux `\` is not a separator
   so the raw backslash form appeared verbatim in the error message.
2. `normalize_file_not_found_message_key` (new) was needed to strip
   machine-specific absolute path prefixes from both the tsz output
   (`/src/harness/...` on Linux) and the cached tsc fingerprints
   (`/var/folders/6z/src/harness/...` on macOS CI) so they converge to the
   same portable form (`src/harness/...`).

Fixes `matchFiles.ts` and `ReferenceResolution` fingerprint-only failures.

## Files Touched

- `crates/tsz-checker/src/state/state_checking/directive.rs` (~3 LOC): normalize `\\` → `/` before `Path::join`
- `crates/conformance/src/tsz_wrapper.rs` (~50 LOC): add `pub(crate) normalize_file_not_found_message_key`, call it from `normalize_message_paths`
- `crates/conformance/src/runner.rs` (~10 LOC): apply normalization to expected cache fingerprints in `filter_lib_diagnostics_tsc`
- `crates/conformance/tests/tsz_wrapper.rs` (~80 LOC): 9 new unit tests

## Verification

- `cargo test -p tsz-conformance -- normalize_file_not_found`: 9/9 pass
- `cargo test -p tsz-conformance`: 130/130 pass
- `conformance.sh run --filter matchFiles`: 1/1 PASS (was fingerprint-only fail)
- `conformance.sh run --filter ReferenceResolution`: 3/3 PASS (was fingerprint-only fail)
- `conformance.sh run --max 200`: 200/200 pass (no regressions)
