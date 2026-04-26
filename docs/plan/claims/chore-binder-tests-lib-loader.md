# chore(binder/tests): expand unit tests for lib_loader.rs

- **Date**: 2026-04-26
- **Time**: 2026-04-26 08:02:44
- **Branch**: `chore/binder-tests-lib-loader`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 8 (DRY/test coverage)

## Intent

Expand the unit-test module for `crates/tsz-binder/src/lib_loader.rs`. The module
currently has only 3 tests covering a small slice of the public surface. Several
public functions and `LibLoader` cache behaviors are entirely uncovered:

- `LibLoader::new`, `load_lib`, `clear_cache`, `cache_size` — disk-backed cache
  helper, zero coverage today.
- `get_suggested_lib_for_type` — version-mapping table, untested.
- `is_es2015_plus_type` — `PromiseLike` special-case branch and the negative
  case for ES5-era globals (`Date`, etc.) are tested in part, but the boundary
  between ES2017/ES2018/ES2020/ES2021/`esnext` is not.
- `emit_error_global_type_missing` and `emit_error_lib_target_mismatch` —
  diagnostic constructors, untested.
- `LibFile::file_locals` — public accessor, untested.

Pure-additive coverage; no behavior changes. Tests are appended to the existing
inline `#[path = "../tests/lib_loader.rs"]` test module.

## Files Touched

- `crates/tsz-binder/tests/lib_loader.rs` (additive, ~150 LOC of new tests)

## Verification

- `cargo nextest run -p tsz-binder -E 'test(lib_loader)'`
